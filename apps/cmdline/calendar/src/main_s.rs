use chrono::naive::{NaiveDate, NaiveDateTime, NaiveTime};
use reedline_repl_rs::clap::{Arg, ArgMatches, Command};
use reedline_repl_rs::Repl;
use reedline_repl_rs::Result as ReplResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tank::client::TankClient;
use tank::data::ScubaData;
use uuid::Uuid;

const ROLES_PREFIX: &str = "roles";
const APPT_PREFIX: &str = "appointment";

// appointments can only be made in 60-minute intervals
const DEFAULT_DUR: u32 = 60;

// TODO impl Display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Provider {
    appointment_ids: Vec<String>,
    availability_id: String,
    //patients: Vec<String>,
}

// TODO impl Display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Patient {
    appointment_ids: Vec<String>,
    //providers: Vec<String>,
}

impl Patient {
    fn new() -> Self {
        Patient {
            appointment_ids: Vec::<String>::new(),
            //providers: Vec::<String>::new(),
        }
    }
}

// TODO impl Display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Roles {
    provider: Option<Provider>,
    patient: Option<Patient>,
}

impl Roles {
    fn new() -> Self {
        Roles {
            provider: None,
            patient: None,
        }
    }
}

/*
 * The AppointmentInfo struct describes each appointment made and is
 * shared between patient and provider
 */

// TODO impl Display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppointmentInfo {
    date: NaiveDate,
    time: NaiveTime,
    duration_min: u32,
    patient_notes: Option<String>,
    pending: bool,
    // TODO add perm field or resolved writer idkeys if patients make apptmts,
    // although data_store doesn't have access to metadata_store, so this would
    // constitute a larger change in Tank (the two are separate for locking
    // purposes now)
}

impl AppointmentInfo {
    fn new(
        date: NaiveDate,
        time: NaiveTime,
        patient_notes: Option<String>,
    ) -> AppointmentInfo {
        AppointmentInfo {
            date,
            time,
            duration_min: DEFAULT_DUR,
            patient_notes,
            pending: true,
        }
    }
}

/*
 * The Availability struct info is shared by providers with their
 * patients. It obfuscates all appointment details or blocked slots and
 * simply shows them all as "busy".
 */

// TODO impl Display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Availability {
    busy_slots: HashMap<NaiveDateTime, u32>,
}

/*
 * Application logic below.
 */

#[derive(Clone)]
struct CalendarApp {
    client: TankClient,
}

impl CalendarApp {
    pub async fn new() -> CalendarApp {
        let client = TankClient::new(
            None, None, false, None, None, false, false, true,
            true, // serializability
            None, None, None, None, None, None, None, None,
            None, // benchmarking args
        )
        .await;
        Self { client }
    }

    pub async fn init_new_device(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        let res = context.client.start_transaction();
        if res.is_err() {
            return Ok(Some(String::from("Cannot start transaction.")));
        }

        context.client.create_standalone_device().await;

        let roles_id = ROLES_PREFIX.to_owned();
        let roles_data = Roles::new();
        let json_string = serde_json::to_string(&roles_data).unwrap();

        match context
            .client
            .set_data(
                roles_id.clone(),
                ROLES_PREFIX.to_string(),
                json_string,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                Ok(Some(String::from("Standalone device created.")))
            }
            Err(err) => {
                context.client.end_transaction().await;
                Ok(Some(String::from(format!(
                    "Could not create device: {}",
                    err.to_string()
                ))))
            }
        }
    }

    pub async fn init_patient_role(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let res = context.client.start_transaction();
        if res.is_err() {
            return Ok(Some(String::from("Cannot start transaction.")));
        }

        match context.client.get_data(&ROLES_PREFIX.to_owned()).await {
            Ok(Some(roles_obj)) => {
                let mut roles: Roles =
                    serde_json::from_str(roles_obj.data_val()).unwrap();

                roles.patient = Some(Patient::new());
                let json_roles = serde_json::to_string(&roles).unwrap();

                match context
                    .client
                    .set_data(
                        ROLES_PREFIX.to_string(),
                        ROLES_PREFIX.to_string(),
                        json_roles,
                        None,
                        None,
                        false,
                    )
                    .await
                {
                    Ok(_) => {
                        context.client.end_transaction().await;
                        Ok(Some(String::from("Created patient role.")))
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        Ok(Some(String::from(format!(
                            "Error creating patient role: {}",
                            err.to_string()
                        ))))
                    }
                }
            }
            Ok(None) => {
                context.client.end_transaction().await;
                Ok(Some(String::from("Roles do not exist.")))
            }
            Err(err) => {
                context.client.end_transaction().await;
                Ok(Some(String::from(format!(
                    "Error getting roles: {}",
                    err.to_string()
                ))))
            }
        }
    }

    fn new_prefixed_id(prefix: &String) -> String {
        let mut id: String = prefix.to_owned();
        id.push_str("/");
        id.push_str(&Uuid::new_v4().to_string());
        id
    }

    // Called by patient
    pub async fn request_appointment(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // TODO check provider_id exists
        let provider_id = args.get_one::<String>("provider_id").unwrap();

        let notes = args.get_one::<String>("notes");

        // parse date
        let date_str = args.get_one::<String>("date").unwrap();
        let date_res = NaiveDate::parse_from_str(date_str, "%Y-%m-%d");
        if date_res.is_err() {
            return Ok(Some(String::from(format!(
                "Error parsing date: {}",
                date_res.err().unwrap().to_string()
            ))));
        }

        // parse time
        let time_str = args.get_one::<String>("time").unwrap();
        let time_res = NaiveTime::parse_from_str(time_str, "%H:%M:%S");
        if time_res.is_err() {
            return Ok(Some(String::from(format!(
                "Error parsing time: {}",
                time_res.err().unwrap().to_string()
            ))));
        }

        let mut res = context.client.start_transaction();
        if res.is_err() {
            return Ok(Some(String::from("Cannot start first transaction.")));
        }

        let appt =
            AppointmentInfo::new(date_res.unwrap(), time_res.unwrap(), notes.cloned());
        let id = Self::new_prefixed_id(&APPT_PREFIX.to_string());
        let json_string = serde_json::to_string(&appt).unwrap();

        // store appointment request
        res = context
            .client
            .set_data(
                id.clone(),
                APPT_PREFIX.to_owned(),
                json_string,
                None,
                None,
                false,
            )
            .await;
        if res.is_err() {
            context.client.end_transaction().await;
            return Ok(Some(String::from(format!(
                "Could not store appointment: {}",
                res.err().unwrap().to_string()
            ))));
        }

        context.client.end_transaction().await;

        // temporary hack b/c cannot set and share data
        // at the same time, and sharing expects that the
        // data already exists, so must wait for set_data
        // message to return from the server
        std::thread::sleep(std::time::Duration::from_secs(1));

        res = context.client.start_transaction();
        if res.is_err() {
            return Ok(Some(String::from("Cannot start second transaction.")));
        }

        // share appointment request with provider
        let vec = vec![provider_id];

        res = context.client.add_writers(id.clone(), vec.clone()).await;
        if res.is_err() {
            context.client.end_transaction().await;
            return Ok(Some(String::from(format!(
                "Could not share appointment: {}",
                res.err().unwrap().to_string()
            ))));
        }

        context.client.end_transaction().await;

        Ok(Some(String::from(format!(
            "Successfully requested appointment with id {}",
            id.clone()
        ))))
    }

    pub async fn get_name(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        let res = context.client.start_transaction();
        if res.is_err() {
            return Ok(Some(String::from("Cannot start transaction.")));
        }

        context.client.end_transaction().await;
        Ok(Some(String::from(format!(
            "Name: {}",
            context.client.linked_name()
        ))))
    }
}

#[tokio::main]
async fn main() -> ReplResult<()> {
    let app = Arc::new(CalendarApp::new().await);

    let mut repl = Repl::new(app.clone())
        .with_name("Calendar App")
        .with_version("v0.1.0")
        .with_description("Scuba calendar app")
        .with_command_async(Command::new("init_new_device"), |_, context| {
            Box::pin(CalendarApp::init_new_device(context))
        })
        .with_command_async(Command::new("init_patient_role"), |_, context| {
            Box::pin(CalendarApp::init_patient_role(context))
        })
        .with_command_async(
            Command::new("request_appointment")
                .arg(
                    Arg::new("provider_id")
                        .required(true)
                        .long("provider_id")
                        .short('p'),
                )
                .arg(
                    Arg::new("date")
                        .required(true)
                        .long("date")
                        .short('d')
                        .help("Format: YYYY-MM-DD"),
                )
                .arg(
                    Arg::new("time")
                        .required(true)
                        .long("time")
                        .short('t')
                        .help("Format: HH:MM:SS (24-hour)"),
                )
                .arg(Arg::new("notes").required(false).long("notes").short('n')),
            |args, context| Box::pin(CalendarApp::request_appointment(args, context)),
        )
        .with_command_async(Command::new("get_name"), |_, context| {
            Box::pin(CalendarApp::get_name(context))
        });
    repl.run_async().await
}
