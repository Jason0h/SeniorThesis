// TODO: find a way for scuba internal code to not panic
// TODO: find a way for artificial waits to not be necessary for code functionality

// TODO: figure out why all data is unavailable after login. solved? (N)
// diagnosis ---> internal issue. data is lost if all devices associated with the id are
// closed. temporary hack: make sure that original device associated with the id is open

// scuba related imports
use tank::client::TankClient;
use tank::data::ScubaData;
// command line imports
use reedline_repl_rs::clap::{Arg, ArgMatches, Command};
use reedline_repl_rs::Repl;
use reedline_repl_rs::Result as ReplResult;
// asynchronous functionality imports
use std::sync::Arc;
// json data conversion imports
use serde::{Deserialize, Serialize};
// time functionality import
use time::{Date, Month, OffsetDateTime, Time};
// coordinate functionality import
use geo::{coord, Coord};
// miscellanious imports
use std::collections::HashMap;
use strum_macros::Display;

// REFERENCE: naming standard for objects in scuba key-value store

// "agent"
// agent: write

// "agent_list"
// coordinator: write. followers: read

// "join_team_request/{agent_alias}"
// follower: write. coordinator: write

// "private_messages/{coordinator_alias}/{agent_alias_from}/{agent_alias_to}"
// agent_from: write. agent_to: read

// "private_messages_info"
// agent_from: write. followers + coordinator: read
// agent: write

// "public_messages/{coordinator_alias}/{agent_alias_from}"
// agent_from: write. followers + coordinator: read

// "public_messages_info"
// agent: write

// "agent_location"
// agent: write

// "location_database/{coordinator_alias}/{agent_alias}"
// agent: write. followers + coordinator: read

// "operation_proposal/{coordinator_alias}/{agent_alias}/{operation_name}"
// agent: write. followers + coordinator: read

// "operation_proposal_vote/{coordinator_alias}/{agent_alias}/{operation_name}/
// {voting_agent_alias}"
// agent: write. followers + coordinator: read

// "committed_operations_list"
// coordinator: write. followers: read

// "operation_proposal/{coordinator_alias}/{agent_alias}/{operation_name}/committed"
// coordinator: write. followers: read

// REFERENCE: how to use debug commands for quick team creation

// note! ccc returns an id name, which you must feed into ca, cb, cc

// creating coordinator (trent) + followers: alice, bob
// window 0: ccc     ab
// window 1:     ca     ch
// window 2:     cb     ch

// creating coordinator (trent) + followers: alice, bob, carol
// window 0: ccc     abc
// window 1:     ca      ch
// window 2:     cb      ch
// window 3:     cc      ch

const AUTO_LOCATION: [f64; 2] = [40.3449, -74.6528];

enum ErrorReturn<T> {
    Error(String),
    Object(T),
}

#[derive(Clone, Serialize, Deserialize, Display)]
enum Role {
    Coordinator,
    Follower,
}

#[derive(Clone, Serialize, Deserialize)]
struct Agent {
    id: String,
    name: String,
    alias: String,
    role: Role,
    coordinator_alias: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct AgentList {
    coordinator: Agent,
    follower_list: HashMap<String, Agent>, // <agent_alias, agent>
}

#[derive(Clone, Serialize, Deserialize, Display)]
enum JoinTeamRequestStatus {
    Active,
    Denied(String),
    Accepted,
}

#[derive(Clone, Serialize, Deserialize)]
struct JoinTeamRequest {
    agent: Agent,
    coordinator_alias: String,
    status: JoinTeamRequestStatus,
}

trait MessageChain {
    fn message_chain(&self) -> Vec<Message>;
    fn agent_from_alias(&self) -> String;
}

impl MessageChain for PrivateMessageChain {
    fn message_chain(&self) -> Vec<Message> {
        return self.message_chain.clone();
    }
    fn agent_from_alias(&self) -> String {
        return self.agent_from_alias.clone();
    }
}

impl MessageChain for PublicMessageChain {
    fn message_chain(&self) -> Vec<Message> {
        return self.message_chain.clone();
    }
    fn agent_from_alias(&self) -> String {
        return self.agent_from_alias.clone();
    }
}

#[derive(Serialize, Deserialize)]
struct PrivateMessagesInfo {
    last_observed_time_stamp_from: HashMap<String, OffsetDateTime>,
    // <agent_alias, time_stamp>
}

#[derive(Serialize, Deserialize, Clone)]
struct PrivateMessageChain {
    agent_from_alias: String,
    agent_to_alias: String,
    message_chain: Vec<Message>,
    last_message_time_stamp: OffsetDateTime,
}

#[derive(Serialize, Deserialize)]
struct PublicMessagesInfo {
    last_observed_time_stamp_from: HashMap<String, OffsetDateTime>,
    // <agent_alias, time_stamp>
}

#[derive(Serialize, Deserialize)]
struct PublicMessageChain {
    agent_from_alias: String,
    message_chain: Vec<Message>,
    last_message_time_stamp: OffsetDateTime,
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time_stamp.cmp(&other.time_stamp)
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.time_stamp == other.time_stamp && self.time_stamp == other.time_stamp
    }
}

impl Eq for Message {}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    message: String,
    time_stamp: OffsetDateTime,
    message_type: MessageType,
}

#[derive(Serialize, Deserialize, Clone, Display)]
enum MessageType {
    Message,
    Alert,
    Announcement,
}

#[derive(Serialize, Deserialize)]
enum AgentLocation {
    Auto,
    Manual(Coord<f64>),
}

#[derive(Serialize, Deserialize)]
struct Location {
    point: Coord<f64>,
    location_type: LocationType,
    info: String,
}

#[derive(Serialize, Deserialize, Display, PartialEq, Clone)]
enum LocationType {
    Resource,
    Danger,
}

#[derive(Serialize, Deserialize)]
struct OperationProposal {
    operation_name: String,
    proposer: String,
    start_time: OffsetDateTime,
    end_time: OffsetDateTime,
    info: String,
}

#[derive(Serialize, Deserialize, Display)]
enum Vote {
    Yes,
    No,
}

// application instance
struct ProtestApp {
    client: TankClient,
}

// application implementation
impl ProtestApp {
    // return an instance of a client (not yet associated with a device)
    async fn new() -> ProtestApp {
        let client = TankClient::new(
            None, None, false, None, None, false, false, true, true, None, None, None,
            None, None, None, None, None, None,
        )
        .await;
        Self { client }
    }

    // PART 0: SHORTCUT COMMANDS (FOR DEBUGGING PURPOSES)

    // for debugging purposes: create a coordinator (i.e. team). return id name
    async fn create_coordinator_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let _ = ProtestApp::signup_agent(String::from("Trent"), context).await;
        std::thread::sleep(std::time::Duration::from_secs(2));
        ProtestApp::create_team(context).await;
        std::thread::sleep(std::time::Duration::from_secs(3));
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(_) => Agent {
                id: String::from(""),
                name: String::from(""),
                alias: String::from(""),
                role: Role::Follower,
                coordinator_alias: None,
            },
        };
        return Ok(Some(String::from(format!("{} {}", agent.id, agent.name))));
    }

    // for debugging purposes: accept alice and bob's requests
    async fn accept_alice_bob_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        ProtestApp::join_team_accept_request(String::from("Alice"), context).await;
        ProtestApp::join_team_accept_request(String::from("Bob"), context).await;
        return Ok(Some(String::from("ab done")));
    }

    // for debugging purposes: accept alice, bob, and carol's requests
    async fn accept_alice_bob_charles_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        ProtestApp::join_team_accept_request(String::from("Alice"), context).await;
        ProtestApp::join_team_accept_request(String::from("Bob"), context).await;
        ProtestApp::join_team_accept_request(String::from("Carol"), context).await;
        return Ok(Some(String::from("abc done")));
    }

    // for debugging purposes: create a follower and send join team request
    async fn create_agent_send_request(
        alias: String,
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let _ = ProtestApp::signup_agent(alias, context).await;
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = ProtestApp::join_team_request_cmd(args, context).await;
        return Ok(Some(String::from("foobar")));
    }

    // for debugging purposes: create a follower and send join team request
    async fn create_alice_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let _ =
            ProtestApp::create_agent_send_request(String::from("Alice"), args, context)
                .await;
        return Ok(Some(String::from("ca done")));
    }

    // for debugging purposes: create a follower and send join team request
    async fn create_bob_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let _ = ProtestApp::create_agent_send_request(String::from("Bob"), args, context)
            .await;
        return Ok(Some(String::from("cb done")));
    }

    // for debugging purposes: create a follower and send join team request
    async fn create_carol_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let _ =
            ProtestApp::create_agent_send_request(String::from("Carol"), args, context)
                .await;
        return Ok(Some(String::from("cc done")));
    }

    // command: for debug purposes: dump all data associated with agent
    async fn dump_data_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::dump_data(context).await {
            ErrorReturn::Object(data) => return Ok(Some(data)),
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: for debug purposes: dump all data associated with agent
    async fn dump_data(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: start transaction & check that device exists
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get and return all data associated with client
        match context.client.get_all_data().await {
            Ok(data) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(itertools::join(data, "\n"));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Device Could Not Be Created. {}",
                    err.to_string()
                )));
            }
        }
    }

    // PART 1: MISCELLANIOUS BASIC FUNCTIONALITY

    // command: create client device and save personal information
    async fn signup_agent_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        ProtestApp::signup_agent(
            args.get_one::<String>("alias").unwrap().to_string(),
            context,
        )
        .await
    }

    // not command: create client device and save personal information
    async fn signup_agent(
        alias: String,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // step 1: start transaction
        let result = context.client.start_transaction();
        if result.is_err() {
            return Ok(Some(String::from(
                "System Error: Unable To Start Transaction",
            )));
        }
        // step 1.1: prevent agent from signing in again if already signed in
        if context.exists_device().await {
            context.client.end_transaction().await;
            return Ok(Some(String::from(
                "Client Error: Client Is Already Signed In, Cannot Sign In Again",
            )));
        }
        // step 2: create client device
        match context.client.create_standalone_device().await {
            Ok(_) => {}
            Err(err) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "System Error: Device Could Not Be Created. {}",
                    err.to_string()
                ))));
            }
        }
        // step a: setup private messages info data in memory
        let private_messages_info = PrivateMessagesInfo {
            last_observed_time_stamp_from: HashMap::new(),
        };
        let json_private_messages_info =
            serde_json::to_string(&private_messages_info).unwrap();

        // step b: commit private messages info data to key value store
        match context
            .client
            .set_data(
                String::from("private_messages_info"),
                String::from("private_messages_info"),
                json_private_messages_info,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {}
            Err(err) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "System Error: Agent Could Not Be Created. {}",
                    err.to_string()
                ))));
            }
        }
        // step a: setup public messages info data in memory
        let public_messages_info = PublicMessagesInfo {
            last_observed_time_stamp_from: HashMap::new(),
        };
        let json_public_messages_info =
            serde_json::to_string(&public_messages_info).unwrap();
        // step b: commit public messages info data to key value store
        match context
            .client
            .set_data(
                String::from("public_messages_info"),
                String::from("public_messages_info"),
                json_public_messages_info,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {}
            Err(err) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "System Error: Agent Could Not Be Created. {}",
                    err.to_string()
                ))));
            }
        }
        // step 3: setup client data in memory
        let agent = Agent {
            id: context.client.idkey(),
            name: context.client.linked_name(),
            alias: alias.clone(),
            role: Role::Follower,
            coordinator_alias: None,
        };
        let json_agent = serde_json::to_string(&agent).unwrap();
        // step 4: commit client data to key value store
        match context
            .client
            .set_data(
                String::from("agent"),
                String::from("agent"),
                json_agent,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "Success: Welcome {}! Please Save Your Id For Future Login: {}",
                    alias.clone(),
                    context.client.idkey()
                ))));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "System Error: Agent Could Not Be Created. {}",
                    err.to_string()
                ))));
            }
        }
    }

    // command: use id as authentication to link client
    async fn login_agent_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        ProtestApp::login_agent(
            args.get_one::<String>("id").unwrap().to_string(),
            context,
        )
        .await
    }

    // not command: use id as authentication to link client
    async fn login_agent(
        id: String,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // step 1: start transaction
        let result = context.client.start_transaction();
        if result.is_err() {
            return Ok(Some(String::from(
                "System Error: Unable To Start Transaction",
            )));
        }
        // step 1.1: prevent agent from logging in again if already logged in
        if context.exists_device().await {
            context.client.end_transaction().await;
            return Ok(Some(String::from(
                "Client Error: Client Is Already Logged In, Cannot Log In Again",
            )));
        }
        // step 2: link client device
        match context.client.create_linked_device(id).await {
            Ok(_) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(3));
                let agent_alias = match ProtestApp::get_agent_alias(context).await {
                    ErrorReturn::Object(agent_alias) => agent_alias,
                    ErrorReturn::Error(err) => {
                        return Ok(Some(String::from(format!(
                            "System Error: Could Not Retrieve Agent Alias. {}",
                            err,
                        ))))
                    }
                };
                return Ok(Some(String::from(format!(
                    "Success: Welcome Back {}!",
                    agent_alias
                ))));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from(format!(
                    "System Error: Device Could Not Be Created. {}",
                    err.to_string()
                ))));
            }
        }
    }

    // helper: returns true is agent is logged in, else false
    async fn exists_device(&self) -> bool {
        match self.client.device.read().as_ref() {
            Some(_) => true,
            None => false,
        }
    }

    // command: return client's identity: i.e. id, name, alias, role
    async fn get_agent_info_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => {
                let coordinator_alias = match agent.coordinator_alias {
                    Some(coordinator_alias) => coordinator_alias,
                    None => String::from("N/A"),
                };
                return Ok(Some(String::from(format!(
                    "Success: Id: {}, Alias: {}, Name: {}, Role: {}, Coordinator Alias: {}",
                    agent.id, agent.alias, agent.name, agent.role, coordinator_alias
                ))));
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: return client's identity: i.e. id, alias, role
    async fn get_agent_info(context: &mut Arc<Self>) -> ErrorReturn<Agent> {
        // step 1: start transaction & check that device exists
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get & return client data from key value store
        match context.client.get_data(&String::from("agent")).await {
            Ok(Some(agent_obj)) => {
                context.client.end_transaction().await;
                let agent: Agent = serde_json::from_str(agent_obj.data_val()).unwrap();
                return ErrorReturn::Object(agent);
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "System Error: Agent Data Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent Data Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
    }

    // command: return client's alias
    async fn get_agent_alias_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_alias(context).await {
            ErrorReturn::Object(agent_alias) => {
                return Ok(Some(String::from(format!(
                    "Success: Your Alias Is {}",
                    agent_alias
                ))));
            }
            ErrorReturn::Error(err) => {
                return Ok(Some(err));
            }
        }
    }

    // not command: return client's alias
    async fn get_agent_alias(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: call get_agent_info and extract agent alias
        let agent_info = ProtestApp::get_agent_info(context).await;
        match agent_info {
            ErrorReturn::Object(agent_info_obj) => {
                return ErrorReturn::Object(agent_info_obj.alias);
            }
            ErrorReturn::Error(err) => {
                return ErrorReturn::Error(err);
            }
        }
    }

    // command: update client's alias
    async fn update_agent_alias_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::update_agent_alias(args, context).await {
            ErrorReturn::Object(new_alias) => {
                return Ok(Some(String::from(format!(
                    "Success: Your Alias Has Been Updated To {}",
                    new_alias
                ))));
            }
            ErrorReturn::Error(err) => {
                return Ok(Some(err));
            }
        }
    }

    // not command: update client's alias
    async fn update_agent_alias(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: get existing client alias from key value store
        let mut agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // step 1.1: check that client has not joined a team
        match agent.coordinator_alias {
            None => {}
            Some(_) => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent Has Already Joined A Team",
                ))
            }
        }
        // TODO: step 1.15: check that client doesn't have a commited join team request
        // step 1.2: check that the client is not yet a coordinator
        match agent.role {
            Role::Follower => {}
            Role::Coordinator => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Coordinators Cannot Update Their Alias",
                ))
            }
        }
        // step 2: set new client alias to key value store
        agent.alias = args.get_one::<String>("alias").unwrap().to_string();
        let json_agent = serde_json::to_string(&agent).unwrap();
        // step 4: commit client data to key value store
        let res = context.client.start_transaction();
        if res.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction.",
            ));
        }
        match context
            .client
            .set_data(
                String::from("agent"),
                String::from("agent"),
                json_agent,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from(agent.alias));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent Could Not be Updated. {}",
                    err.to_string()
                )));
            }
        }
        // TODO: step 4.5: if client has an active join team request, update alias
    }

    // command: return client id
    async fn get_agent_id_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        // step 1: start transaction & check that device exists
        let result = context.client.start_transaction();
        if result.is_err() {
            context.client.end_transaction().await;
            return Ok(Some(String::from(
                "System Error: Unable To Start Transaction",
            )));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return Ok(Some(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            )));
        }
        // step 2: return client id
        context.client.end_transaction().await;
        Ok(Some(String::from(format!(
            "Success: Your Id Is: {}",
            context.client.idkey()
        ))))
    }

    // command: return client's name
    async fn get_agent_name_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_name(context).await {
            ErrorReturn::Object(name) => {
                return Ok(Some(String::from(format!(
                    "Success: Your Name Is: {}",
                    name
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: return client's coordinator's id
    async fn get_agent_name(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get and return client's name
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => return ErrorReturn::Object(agent.name),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // command: return client's coordinator's alias
    async fn get_agent_coordinator_alias_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_coordinator_alias(context).await {
            ErrorReturn::Object(coordinator_alias) => {
                return Ok(Some(String::from(format!(
                    "Success: Coordinator's Alias Is: {}",
                    coordinator_alias
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: return client's coordinator's alias
    async fn get_agent_coordinator_alias(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get and return client's coordinator's alias
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => match agent.role {
                Role::Follower => {
                    let coordinator_alias = match agent.coordinator_alias {
                        Some(coordinator_alias) => coordinator_alias,
                        None => String::from("N/A"),
                    };
                    ErrorReturn::Object(coordinator_alias)
                }
                Role::Coordinator => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Agent is a Coordinator",
                    ))
                }
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // command: return client's role
    async fn get_agent_role_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_role(context).await {
            ErrorReturn::Object(role) => {
                return Ok(Some(String::from(format!(
                    "Success: Your Role Is: {}",
                    role
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: return client's role
    async fn get_agent_role(context: &mut Arc<Self>) -> ErrorReturn<Role> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get and return client's role
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => return ErrorReturn::Object(agent.role),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // PART 2: TEAM CREATION & JOINING FUNCTIONALITY

    // helper: format agent list into a string of aliases
    fn agent_list_to_aliases_str(agent_list: &AgentList) -> String {
        let mut agent_list_str = String::from("");
        // step 1: add coordinator information to string
        agent_list_str.push_str(&String::from("Coordinator:\n"));
        let coordinator = &agent_list.coordinator;
        agent_list_str.push_str(&String::from(format!("{}", &coordinator.alias)));
        agent_list_str.push_str(&String::from("\n"));
        // step 2: add follower information to string
        agent_list_str.push_str(&String::from("\nFollowers:\n"));
        for (alias, _agent) in &agent_list.follower_list {
            agent_list_str.push_str(&String::from(format!("{}\n", alias)));
        }
        agent_list_str.pop();
        return agent_list_str;
    }

    // command: retrieve agent aliases list (must be coordinator or have joined a team)
    async fn get_agent_alias_list_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => {
                return Ok(Some(ProtestApp::agent_list_to_aliases_str(&agent_list)))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // helper: format agent list into a string
    fn agent_list_to_str(agent_list: &AgentList) -> String {
        let mut agent_list_str = String::from("");
        // step 1: add coordinator information to string
        agent_list_str.push_str(&String::from("Coordinator:\n"));
        let coordinator = &agent_list.coordinator;
        agent_list_str.push_str(&String::from(format!(
            "Alias: {}, Id: {}, Name: {}\n",
            &coordinator.alias, &coordinator.id, &coordinator.name,
        )));
        agent_list_str.push_str(&String::from("\n"));
        // step 2: add follower information to string
        agent_list_str.push_str(&String::from("Followers:\n"));
        for (alias, agent) in &agent_list.follower_list {
            agent_list_str.push_str(&String::from(format!(
                "Alias: {}, Id: {}, Name: {}\n",
                alias, &agent.id, &agent.name
            )));
        }
        agent_list_str.pop();
        return agent_list_str;
    }

    // command: retrieve agent list (must be coordinator or have joined a team)
    async fn get_agent_list_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => {
                return Ok(Some(ProtestApp::agent_list_to_str(&agent_list)))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: retrieve agent list (must be coordinator or have joined a team)
    async fn get_agent_list(context: &mut Arc<Self>) -> ErrorReturn<AgentList> {
        // step 1: start transaction & check that device exists
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get & return agent list from key value store
        match context.client.get_data(&String::from("agent_list")).await {
            Ok(Some(agent_list_obj)) => {
                context.client.end_transaction().await;
                let agent_list: AgentList =
                    serde_json::from_str(agent_list_obj.data_val()).unwrap();
                return ErrorReturn::Object(agent_list);
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent List Does Not Exist. Create Team Or Join Team First",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent List Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
    }

    // command: promote agent to coordinator and create agent list
    async fn create_team_cmd(context: &mut Arc<Self>) -> ReplResult<Option<String>> {
        match ProtestApp::create_team(context).await {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Team Has Been Created. Agents May Send Join Team Requests To Your Id Name",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: promote agent to coordinator and create agent list
    async fn create_team(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: abort if agent is already a coordinator (already has team)
        match ProtestApp::get_agent_role(context).await {
            ErrorReturn::Object(role) => match role {
                Role::Follower => {}
                Role::Coordinator => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Agent is Already a Coordinator",
                    ))
                }
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 3: create agent list in memory
        let mut agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let agent_list = AgentList {
            coordinator: agent.clone(),
            follower_list: HashMap::new(),
        };
        let json_agent_list = serde_json::to_string(&agent_list).unwrap();
        // step 4: commit agent list to key value store
        let res = context.client.start_transaction();
        if res.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Cannot Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from("agent_list"),
                String::from("agent_list"),
                json_agent_list,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => context.client.end_transaction().await,
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent List Could Not be Created. {}",
                    err.to_string()
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step a: create committed operations list in memory
        let committed_operations_list: Vec<OperationProposal> = Vec::new();
        let json_committed_operations_list =
            serde_json::to_string(&committed_operations_list).unwrap();
        // step b: commit commited operations list to key value store
        let res = context.client.start_transaction();
        if res.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Cannot Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from("committed_operations_list"),
                String::from("committed_operations_list"),
                json_committed_operations_list,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => context.client.end_transaction().await,
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Operations List Could Not be Created. {}",
                    err.to_string()
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 5: promote own role to coordinator
        agent.role = Role::Coordinator;
        agent.coordinator_alias = Some(agent.alias.clone());
        let json_agent = serde_json::to_string(&agent).unwrap();
        let res = context.client.start_transaction();
        if res.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Cannot Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from("agent"),
                String::from("agent"),
                json_agent,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => context.client.end_transaction().await,
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent Could Not be Updated. {}",
                    err.to_string()
                )));
            }
        }
        return ErrorReturn::Object(String::from("Success:"));
    }

    // command: create a join team request with shared Follower - Coordinator access
    async fn join_team_request_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::join_team_request(args, context).await {
            ErrorReturn::Object(coordinator_id) => {
                return Ok(Some(String::from(format!(
                    "Success: Join Team Request Has Been Created And Sent To {}",
                    coordinator_id
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: create a join team request with shared Follower - Coordinator access
    async fn join_team_request(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: check that agent is not a Coordinator
        match ProtestApp::get_agent_role(context).await {
            ErrorReturn::Object(role) => match role {
                Role::Follower => {}
                Role::Coordinator => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Coordinator Can Not Make A Join Team Request",
                    ))
                }
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        // step 3: create a join team request in memory
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let coordinator_id = args
            .get_one::<String>("coordinator_id")
            .unwrap()
            .to_string();
        let join_team_request = JoinTeamRequest {
            agent: agent.clone(),
            coordinator_alias: String::from("N/A"),
            status: JoinTeamRequestStatus::Active,
        };
        let json_join_team_request = serde_json::to_string(&join_team_request).unwrap();
        // step 2.1: check that agent has not joined a team
        match agent.coordinator_alias {
            Some(_) => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent Already Joined A Team, Cannot Make A Join Team Request",
                ));
            }
            None => {}
        }

        // TODO?/NOTE: we do not check if agent sent a join team request. instead, they
        // are overwritten. in the case that multiple requests are sent, agent
        // will be a ghost membor of prior teams

        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 4: commit join team request to key value store
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from(format!("join_team_request/{}", agent.alias)),
                String::from(format!("join_team_request")),
                json_join_team_request,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Join Team Request Could Not Be Created. {}",
                    err.to_string()
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 5.0: establish contact with coordinator
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.add_contact(coordinator_id.clone()).await {
            Ok(_) => context.client.end_transaction().await,
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Unable To Add Coordinator As Contact. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 5: share join team request
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let coordinator_name = args.get_one::<String>("coordinator_name").unwrap();
        let writers = vec![coordinator_name];
        match context
            .client
            .add_writers(
                String::from(format!("join_team_request/{}", agent.alias)),
                writers.clone(),
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from("Coordinator"));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Join Team Request Could Not Be Shared. {}",
                    err.to_string()
                )));
            }
        }
    }

    // command: accept a Follower's join team request
    async fn join_team_accept_request_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::join_team_accept_request(
            args.get_one::<String>("agent_alias").unwrap().to_string(),
            context,
        )
        .await
        {
            ErrorReturn::Object(agent_alias) => {
                return Ok(Some(String::from(format!(
                    "Success: Join Team Request Of {} Has Been Accepted",
                    agent_alias
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: accept a Follower's join team request
    async fn join_team_accept_request(
        agent_alias: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 1.5: check that agent is a Coordinator, not a Follower
        let coordinator_alias: String;
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => match agent.role {
                Role::Follower => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Followers Cannot Accept Join Team Requests",
                    ))
                }
                Role::Coordinator => {
                    coordinator_alias = agent.alias;
                }
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // note: keep track of reason (or no reason) to reject join team request
        let mut status = JoinTeamRequestStatus::Accepted;
        // step 2: retrieve agent's join team request (if it exists).
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let data_id = String::from(format!("join_team_request/{}", agent_alias));
        let mut join_team_request = match context.client.get_data(&data_id).await {
            Ok(Some(join_team_request_obj)) => {
                context.client.end_transaction().await;
                let join_team_request: JoinTeamRequest =
                    serde_json::from_str(join_team_request_obj.data_val()).unwrap();
                // don't reconsider already rejected
                match join_team_request.status {
                    JoinTeamRequestStatus::Denied(_) => return ErrorReturn::Error(String::from(
                        "Client Error: This Join Team Request Has Already Been Denied",
                    )),
                    JoinTeamRequestStatus::Accepted => {}
                    JoinTeamRequestStatus::Active => {}
                }
                // important! note: no two agents in a team can share the same alias
                std::thread::sleep(std::time::Duration::from_secs(1));
                match ProtestApp::get_agent_list(context).await {
                    ErrorReturn::Object(agent_list) => {
                        if agent_alias == agent_list.coordinator.alias {
                            status = JoinTeamRequestStatus::Denied(String::from(
                                "Client Error: Your Alias Is Already In Use. Update Your Alias"
                            ))
                        }
                        for (follower_alias, _agent) in &agent_list.follower_list {
                            if agent_alias == *follower_alias {
                                status = JoinTeamRequestStatus::Denied(String::from(
                                    "Client Error: Your Alias Is Already In Use. Update Your Alias"
                                ))
                            }
                        }
                    }
                    ErrorReturn::Error(err) => return ErrorReturn::Error(err),
                }
                join_team_request
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "Client Error: Join Team Request From {} Does Not Exist",
                    agent_alias
                )));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent Data Could Not Be Retrieved. {}",
                    err
                )));
            }
        };
        // step 3: add agent to team (if there is no reason to reject request)
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        join_team_request.status = status.clone();
        join_team_request.coordinator_alias = coordinator_alias;
        let json_join_team_request = serde_json::to_string(&join_team_request).unwrap();
        match context
            .client
            .set_data(
                String::from(format!("join_team_request/{}", agent_alias)),
                String::from(format!("join_team_request")),
                json_join_team_request,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Join Team Request Could Not Be Created. {}",
                    err.to_string()
                )));
            }
        }
        match status {
            JoinTeamRequestStatus::Accepted => {
                // step 3.5: add accepted agent to agent list
                std::thread::sleep(std::time::Duration::from_secs(1));
                let mut agent_list = match ProtestApp::get_agent_list(context).await {
                    ErrorReturn::Object(agent_list) => agent_list,
                    ErrorReturn::Error(err) => return ErrorReturn::Error(err),
                };
                std::thread::sleep(std::time::Duration::from_secs(1));
                agent_list
                    .follower_list
                    .insert(agent_alias.clone(), join_team_request.agent.clone());
                let json_agent_list = serde_json::to_string(&agent_list).unwrap();
                let res = context.client.start_transaction();
                if res.is_err() {
                    return ErrorReturn::Error(String::from(
                        "System Error: Cannot Start Transaction",
                    ));
                }
                match context
                    .client
                    .set_data(
                        String::from("agent_list"),
                        String::from("agent_list"),
                        json_agent_list,
                        None,
                        None,
                        false,
                    )
                    .await
                {
                    Ok(_) => context.client.end_transaction().await,
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Agent List Could Not be Updated. {}",
                            err.to_string()
                        )));
                    }
                }
                // step 3.6: share agent list (as a reader) with new agent
                // and share committed operations list
                std::thread::sleep(std::time::Duration::from_secs(1));
                let reader = join_team_request.agent.name.clone();
                let readers = vec![&reader];
                match context
                    .client
                    .add_do_readers(String::from("agent_list"), readers.clone())
                    .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Agent List Could Not Be Shared. {}",
                            err
                        )));
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                match context
                    .client
                    .add_do_readers(String::from("committed_operations_list"), readers)
                    .await
                {
                    Ok(_) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Object(String::from(format!(
                            "{}",
                            agent_alias
                        )));
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Agent List Could Not Be Shared. {}",
                            err
                        )));
                    }
                }
            }
            JoinTeamRequestStatus::Denied(_) => {
                return ErrorReturn::Object(String::from(
                    "Client Error: Provided Agent Alias Is Already In Use In The Team",
                ))
            }
            JoinTeamRequestStatus::Active => {
                return ErrorReturn::Error(String::from(
                    "System Error: This Line Of Code Should be Unreachable",
                ));
            }
        }
    }

    // command: remove agent from agent list (informing other agents to stop comms)
    async fn remove_from_team_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_alias = args.get_one::<String>("agent_alias").unwrap().to_string();
        match ProtestApp::remove_from_team(agent_alias.clone(), context).await {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(format!(
                    "Success: {} Removed From Team",
                    agent_alias
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: remove agent from agent list (informing other agents to stop comms)
    async fn remove_from_team(
        agent_alias: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: check that self is a coordinator first
        let agent_role = match ProtestApp::get_agent_role(context).await {
            ErrorReturn::Object(agent_role) => agent_role,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        match agent_role {
            Role::Coordinator => {}
            Role::Follower => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Followers May Not Modify Agent List",
                ))
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 1: get agent list
        let mut agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // step 2: update agent list
        std::thread::sleep(std::time::Duration::from_secs(1));
        match agent_list.follower_list.remove(&agent_alias) {
            Some(_) => {}
            None => {
                return ErrorReturn::Error(String::from(format!(
                    "Client Error: {} Is Not In The Agent List",
                    agent_alias
                )))
            }
        }
        let json_agent_list = serde_json::to_string(&agent_list).unwrap();
        let res = context.client.start_transaction();
        if res.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Cannot Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from("agent_list"),
                String::from("agent_list"),
                json_agent_list,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => context.client.end_transaction().await,
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent List Could Not be Updated. {}",
                    err.to_string()
                )));
            }
        }
        return ErrorReturn::Object(String::from(""));
    }

    // command: check if coordinator accepted request. necessary to fully join team
    async fn check_join_team_request_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::check_join_team_request(context).await {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Join Team Request Has Been Accepted",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: check if coordinator accepted request. necessary to fully join team
    async fn check_join_team_request(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let alias = match ProtestApp::get_agent_alias(context).await {
            ErrorReturn::Object(alias) => alias,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: check status of join team request
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .get_data(&String::from(format!("join_team_request/{}", alias)))
            .await
        {
            Ok(Some(join_team_request_obj)) => {
                context.client.end_transaction().await;
                // step 2.5: update agent if join team request was accepted
                let join_team_request: JoinTeamRequest =
                    serde_json::from_str(join_team_request_obj.data_val()).unwrap();
                match join_team_request.status {
                    JoinTeamRequestStatus::Accepted => {
                        let mut agent = match ProtestApp::get_agent_info(context).await {
                            ErrorReturn::Object(agent) => agent,
                            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
                        };
                        agent.coordinator_alias =
                            Some(join_team_request.coordinator_alias);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        let result = context.client.start_transaction();
                        if result.is_err() {
                            return ErrorReturn::Error(String::from(
                                "System Error: Unable To Start Transaction",
                            ));
                        }
                        let json_agent = serde_json::to_string(&agent).unwrap();
                        match context
                            .client
                            .set_data(
                                String::from("agent"),
                                String::from("agent"),
                                json_agent,
                                None,
                                None,
                                false,
                            )
                            .await
                        {
                            Ok(_) => {
                                context.client.end_transaction().await;
                                return ErrorReturn::Object(String::from(""));
                            }
                            Err(err) => {
                                context.client.end_transaction().await;
                                return ErrorReturn::Error(String::from(format!(
                                    "System Error: Agent Could Not Be updated. {}",
                                    err.to_string()
                                )));
                            }
                        }
                    }
                    JoinTeamRequestStatus::Active => {
                        return ErrorReturn::Error(String::from(
                            "Pending: Join Team Request Has Not Been Accepted Yet",
                        ));
                    }
                    JoinTeamRequestStatus::Denied(err) => {
                        return ErrorReturn::Error(err);
                    }
                }
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "System Error: Join Team Request Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Join Team Request Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
    }

    // PART 3: PRIVATE MESSAGING FUNCTIONALITY

    // helper: convert MessageType string to MessageType enum
    fn str_to_message_type(message_type_str: String) -> MessageType {
        if message_type_str == "Message".to_string() {
            return MessageType::Message;
        } else if message_type_str == "Alert".to_string() {
            return MessageType::Alert;
        } else if message_type_str == "Announcement".to_string() {
            return MessageType::Announcement;
        } else {
            return MessageType::Message;
        }
    }

    // helper: get the current time (isolated for hot swappable purposes)
    async fn get_time() -> OffsetDateTime {
        return OffsetDateTime::now_utc();
    }

    // command: send private message to a team member. message is associated with team
    async fn send_private_message_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_to_alias = args
            .get_one::<String>("agent_to_alias")
            .unwrap()
            .to_string();
        let message = args.get_one::<String>("message").unwrap().to_string();
        let message_type = args.get_one::<String>("message_type");
        let message_type = match message_type {
            Some(message_type_str) => message_type_str.to_string(),
            None => String::from(""),
        };
        let message_type = ProtestApp::str_to_message_type(message_type);
        match ProtestApp::send_private_message(
            agent_to_alias.clone(),
            message,
            message_type,
            context,
        )
        .await
        {
            ErrorReturn::Object(time) => {
                return Ok(Some(String::from(format!(
                    "Success: Message Sent To {} At Time {}",
                    agent_to_alias, time
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: send private message to a team member. message associated with team
    async fn send_private_message(
        agent_to_alias: String,
        message: String,
        message_type: MessageType,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<OffsetDateTime> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2.0: prepare a message to append
        let new_message = Message {
            message,
            time_stamp: ProtestApp::get_time().await,
            message_type,
        };
        let time_stamp = new_message.time_stamp.clone();
        // step 2: does private messages vector exist? if not, then create & share one
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // aside: check that agent is not sending private message to himself
        if agent.alias == agent_to_alias {
            return ErrorReturn::Error(String::from(
                "Client Error: Agent Cannot Send Private Message To Themself",
            ));
        }
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.unwrap(),
            agent.alias,
            agent_to_alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let mut private_messages: PrivateMessageChain;
        let mut created_new_chain = false;
        match context.client.get_data(&data_id.clone()).await {
            // step 2.1: in this case, append to existing message chain
            Ok(Some(private_messages_object)) => {
                private_messages =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                // make a just in case check: new message time stamp doesn't violate
                // message chain's time stamp (i.e. it's strictly greater)
                if !(new_message.time_stamp > private_messages.last_message_time_stamp) {
                    return ErrorReturn::Error(String::from("System Error: New Message's Timestamp Violates Time Invariant of Message Chain"));
                }
                private_messages.message_chain.push(new_message.clone());
                // OMG THIS WAS THE BUG!!!!!!!!!! I FORGOT TO ADD THIS LINE!!!!!!!!!!
                private_messages.last_message_time_stamp = new_message.time_stamp.clone();
            }
            // step 2.2: in this case, create a new message chain
            Ok(None) => {
                private_messages = PrivateMessageChain {
                    agent_from_alias: agent.alias,
                    agent_to_alias: agent_to_alias.clone(),
                    last_message_time_stamp: new_message.time_stamp.clone(),
                    message_chain: vec![new_message],
                };
                created_new_chain = true
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        context.client.end_transaction().await;
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        // step 3: commit message chain to key value store
        let json_private_messages = serde_json::to_string(&private_messages).unwrap();
        match context
            .client
            .set_data(
                data_id.clone(),
                String::from(format!("private_messages")),
                json_private_messages,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 4: share message chain with message recipient
        let agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        let reader = match agent_list.follower_list.get(&agent_to_alias) {
            Some(agent) => agent.clone(),
            None => {
                if agent_list.coordinator.alias == agent_to_alias {
                    agent_list.coordinator
                } else {
                    return ErrorReturn::Error(String::from(format!(
                        "Client Error: {} Is Not A Part Of The Team",
                        agent_to_alias.clone()
                    )));
                }
            }
        };
        if created_new_chain {
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
            match context.client.add_contact(reader.id.clone()).await {
                Ok(_) => {
                    context.client.end_transaction().await;
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Unable To Add Agent To As Contact. {}",
                        err
                    )));
                }
            }
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
            let readers = vec![&reader.name];
            match context.client.add_do_readers(data_id, readers).await {
                Ok(_) => {
                    context.client.end_transaction().await;
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Message Chain Could Not Be Shared. {}",
                        err
                    )));
                }
            }
        }
        return ErrorReturn::Object(time_stamp);
    }

    // helper: returns formatted offset dat time
    fn format_offset_date_time(time: OffsetDateTime) -> String {
        String::from(format!(
            "{}-{}-{} {}:{}:{}",
            time.year(),
            time.month() as u32,
            time.day(),
            time.hour(),
            time.minute(),
            time.second()
        ))
    }

    // helper: returns formatted messages for the command line
    fn format_message_chains<T: MessageChain>(
        self_message_chain: Option<T>,
        other_message_chains: Vec<T>,
        num_last_messages: Option<u32>,
    ) -> String {
        // part 1: combine all messages into a vector (plus hacky formatting steps too)
        let mut messages_vector: Vec<Message> = Vec::new();
        match self_message_chain {
            Some(self_message_chain) => {
                let mut count: u32 = 0;
                for message in &mut self_message_chain.message_chain() {
                    let to_prepend = String::from(format!(
                        "{} {} {}: {}: ",
                        count,
                        ProtestApp::format_offset_date_time(message.time_stamp),
                        message.message_type,
                        self_message_chain.agent_from_alias(),
                    ));
                    let to_prepend = format!("{:>45}", to_prepend);
                    message.message.insert_str(0, &to_prepend);
                    count += 1;
                    messages_vector.push(message.clone());
                }
            }
            None => {}
        }
        for message_chain in other_message_chains {
            for mut message in message_chain.message_chain() {
                let to_prepend = String::from(format!(
                    "{} {}: {}: ",
                    ProtestApp::format_offset_date_time(message.time_stamp),
                    message.message_type,
                    message_chain.agent_from_alias(),
                ));
                let to_prepend = format!("{:>45}", to_prepend);
                message.message.insert_str(0, &to_prepend);
                messages_vector.push(message.clone());
            }
        }
        // part 2: sort the messages (by increasing time)
        messages_vector.sort();
        // part 3: leave only num_last_messages of messages in the vector
        match num_last_messages {
            Some(num_last_messages) => {
                let num_last_messages: usize = num_last_messages as usize;
                if messages_vector.len() > num_last_messages {
                    let start_idx = messages_vector.len() - num_last_messages;
                    messages_vector = messages_vector[start_idx..].to_vec();
                }
            }
            None => {}
        }
        // part 4: convert the messages vector into a string to return
        let mut messages = String::from("");
        for message in messages_vector {
            messages.push_str(&String::from(format!("{}\n", message.message)));
        }
        messages.pop();
        return messages;
    }

    // command: get message chain with an agent. optionally get last num messages
    async fn get_private_messages_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_to_alias = args
            .get_one::<String>("agent_to_alias")
            .unwrap()
            .to_string();
        let num_last_messages = args.get_one::<String>("num_last_messages");
        let num_last_messages: Option<u32> =
            num_last_messages.map(|s| s.parse::<u32>().ok()).flatten();
        match ProtestApp::get_private_messages(agent_to_alias, num_last_messages, context)
            .await
        {
            ErrorReturn::Object(result) => {
                return Ok(Some(String::from(format!("{}", result))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: get message chain with an agent. optionally get last num messages
    async fn get_private_messages(
        agent_to_alias: String,
        num_last_messages: Option<u32>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let mut both_nonexistent = true;
        // step 1: get agent_self to agent_to messages
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias,
            agent_to_alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let self_to_message_chain: Option<PrivateMessageChain>;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(private_messages_object)) => {
                context.client.end_transaction().await;
                let private_messages =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                self_to_message_chain = Some(private_messages);
                both_nonexistent = false;
            }
            Ok(None) => {
                context.client.end_transaction().await;
                self_to_message_chain = None
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: get agent_to to agent_self messages
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent_to_alias,
            agent.alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let to_self_message_chain: Option<PrivateMessageChain>;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(private_messages_object)) => {
                context.client.end_transaction().await;
                let private_messages: PrivateMessageChain =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                to_self_message_chain = Some(private_messages.clone());
                both_nonexistent = false;
                // step 2.5: update private messages info with new timestamp
                std::thread::sleep(std::time::Duration::from_secs(1));
                let new_timestamp = private_messages.last_message_time_stamp;
                let mut new_private_messages_info: PrivateMessagesInfo;
                // get private messages info from key value store & update
                let result = context.client.start_transaction();
                if result.is_err() {
                    return ErrorReturn::Error(String::from(
                        "System Error: Unable To Start Transaction",
                    ));
                }
                match context
                    .client
                    .get_data(&String::from("private_messages_info"))
                    .await
                {
                    Ok(Some(private_messages_info_object)) => {
                        context.client.end_transaction().await;
                        let private_messages_info: PrivateMessagesInfo =
                            serde_json::from_str(private_messages_info_object.data_val())
                                .unwrap();
                        new_private_messages_info = private_messages_info;
                    }
                    Ok(None) => {
                        context.client.end_transaction().await;
                        new_private_messages_info = PrivateMessagesInfo {
                            last_observed_time_stamp_from: HashMap::new(),
                        }
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Private Messages Info Could Not Be Retrieved. {}",
                            err
                        )));
                    }
                }
                new_private_messages_info
                    .last_observed_time_stamp_from
                    .insert(agent_to_alias.clone(), new_timestamp.clone());
                // commit private messages info to key value store
                std::thread::sleep(std::time::Duration::from_secs(1));
                let result = context.client.start_transaction();
                if result.is_err() {
                    return ErrorReturn::Error(String::from(
                        "System Error: Unable To Start Transaction",
                    ));
                }
                let json_private_messages_info =
                    serde_json::to_string(&new_private_messages_info).unwrap();
                match context
                    .client
                    .set_data(
                        String::from("private_messages_info"),
                        String::from("private_messages_info"),
                        json_private_messages_info,
                        None,
                        None,
                        false,
                    )
                    .await
                {
                    Ok(_) => {
                        context.client.end_transaction().await;
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Private Messages Info Could Not Be Updated. {}",
                            err.to_string()
                        )));
                    }
                }
            }
            Ok(None) => {
                context.client.end_transaction().await;
                to_self_message_chain = None
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        let vec_to_self_message_chain = match to_self_message_chain {
            Some(unwrapped_message_chain) => {
                vec![unwrapped_message_chain]
            }
            None => Vec::new(),
        };
        // step 3: return formatted messages
        if both_nonexistent {
            return ErrorReturn::Error(String::from(format!(
                "Client Error: There Are No Messages Between {} And {}",
                agent.alias, agent_to_alias
            )));
        }
        return ErrorReturn::Object(ProtestApp::format_message_chains(
            self_to_message_chain,
            vec_to_self_message_chain,
            num_last_messages,
        ));
    }

    // helper: returns formatted messages for the command line
    fn format_message_chains_t<T: MessageChain>(
        self_message_chain: Option<T>,
        other_message_chains: Vec<T>,
        greater_than_timestamp: Option<OffsetDateTime>,
    ) -> String {
        // part 1: combine all messages into a vector (plus hacky formatting steps too)
        let mut messages_vector: Vec<Message> = Vec::new();
        match self_message_chain {
            Some(self_message_chain) => {
                let mut count: u32 = 0;
                for message in &mut self_message_chain.message_chain() {
                    let to_prepend = String::from(format!(
                        "{} {} {}: {}: ",
                        count,
                        ProtestApp::format_offset_date_time(message.time_stamp),
                        message.message_type,
                        self_message_chain.agent_from_alias(),
                    ));
                    let to_prepend = format!("{:>45}", to_prepend);
                    message.message.insert_str(0, &to_prepend);
                    count += 1;
                    messages_vector.push(message.clone());
                }
            }
            None => {}
        }
        for message_chain in other_message_chains {
            for mut message in message_chain.message_chain() {
                let to_prepend = String::from(format!(
                    "{} {}: {}: ",
                    ProtestApp::format_offset_date_time(message.time_stamp),
                    message.message_type,
                    message_chain.agent_from_alias(),
                ));
                let to_prepend = format!("{:>45}", to_prepend);
                message.message.insert_str(0, &to_prepend);
                messages_vector.push(message.clone());
            }
        }
        // part 2: sort the messages (by increasing time)
        messages_vector.sort();
        // part 3: leave only num_last_messages of messages in the vector
        match greater_than_timestamp {
            Some(greater_than_timestamp) => {
                messages_vector = messages_vector
                    .into_iter()
                    .filter(|message| message.time_stamp > greater_than_timestamp)
                    .collect();
            }
            None => {}
        }
        // part 4: convert the messages vector into a string to return
        let mut messages = String::from("");
        for message in messages_vector {
            messages.push_str(&String::from(format!("{}\n", message.message)));
        }
        messages.pop();
        return messages;
    }

    // command: get message chain with an agent. messages are since last time checked
    async fn get_new_private_messages_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_to_alias = args
            .get_one::<String>("agent_to_alias")
            .unwrap()
            .to_string();
        match ProtestApp::get_new_private_messages(agent_to_alias, context).await {
            ErrorReturn::Object(result) => {
                return Ok(Some(String::from(format!("{}", result))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: get message chain with an agent. messages are since last time checked
    async fn get_new_private_messages(
        agent_to_alias: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let mut both_nonexistent = true;
        // step a: what is the last message you've viewed from agent_to?
        let mut time_last_message_viewed = None;
        // step 1: get agent_self to agent_to messages
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias,
            agent_to_alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let self_to_message_chain: Option<PrivateMessageChain>;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(private_messages_object)) => {
                context.client.end_transaction().await;
                let private_messages =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                self_to_message_chain = Some(private_messages);
                both_nonexistent = false;
            }
            Ok(None) => {
                context.client.end_transaction().await;
                self_to_message_chain = None
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: get agent_to to agent_self messages
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent_to_alias,
            agent.alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let to_self_message_chain: Option<PrivateMessageChain>;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(private_messages_object)) => {
                context.client.end_transaction().await;
                let private_messages: PrivateMessageChain =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                to_self_message_chain = Some(private_messages.clone());
                both_nonexistent = false;
                // step 2.5: update private messages info with new timestamp
                std::thread::sleep(std::time::Duration::from_secs(1));
                let new_timestamp = private_messages.last_message_time_stamp;
                let mut new_private_messages_info: PrivateMessagesInfo;
                // get private messages info from key value store & update
                let result = context.client.start_transaction();
                if result.is_err() {
                    return ErrorReturn::Error(String::from(
                        "System Error: Unable To Start Transaction",
                    ));
                }
                match context
                    .client
                    .get_data(&String::from("private_messages_info"))
                    .await
                {
                    Ok(Some(private_messages_info_object)) => {
                        context.client.end_transaction().await;
                        let private_messages_info: PrivateMessagesInfo =
                            serde_json::from_str(private_messages_info_object.data_val())
                                .unwrap();
                        new_private_messages_info = private_messages_info;
                        // step a: what is the last message you've viewed from agent_to?
                        time_last_message_viewed = match new_private_messages_info
                            .last_observed_time_stamp_from
                            .get(&agent_to_alias.clone())
                        {
                            Some(time_stamp) => {
                                let time_stamp = (*time_stamp).clone();
                                Some(time_stamp)
                            }
                            None => None,
                        };
                    }
                    Ok(None) => {
                        context.client.end_transaction().await;
                        new_private_messages_info = PrivateMessagesInfo {
                            last_observed_time_stamp_from: HashMap::new(),
                        }
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Private Messages Info Could Not Be Retrieved. {}",
                            err
                        )));
                    }
                }
                new_private_messages_info
                    .last_observed_time_stamp_from
                    .insert(agent_to_alias.clone(), new_timestamp.clone());
                // commit private messages info to key value store
                std::thread::sleep(std::time::Duration::from_secs(1));
                let result = context.client.start_transaction();
                if result.is_err() {
                    return ErrorReturn::Error(String::from(
                        "System Error: Unable To Start Transaction",
                    ));
                }
                let json_private_messages_info =
                    serde_json::to_string(&new_private_messages_info).unwrap();
                match context
                    .client
                    .set_data(
                        String::from("private_messages_info"),
                        String::from("private_messages_info"),
                        json_private_messages_info,
                        None,
                        None,
                        false,
                    )
                    .await
                {
                    Ok(_) => {
                        context.client.end_transaction().await;
                    }
                    Err(err) => {
                        context.client.end_transaction().await;
                        return ErrorReturn::Error(String::from(format!(
                            "System Error: Private Messages Info Could Not Be Updated. {}",
                            err.to_string()
                        )));
                    }
                }
            }
            Ok(None) => {
                context.client.end_transaction().await;
                to_self_message_chain = None
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        let vec_to_self_message_chain = match to_self_message_chain {
            Some(unwrapped_message_chain) => {
                vec![unwrapped_message_chain]
            }
            None => Vec::new(),
        };
        // step 3: return formatted messages
        if both_nonexistent {
            return ErrorReturn::Error(String::from(format!(
                "Client Error: There Are No Messages Between {} And {}",
                agent.alias, agent_to_alias
            )));
        }
        // print!("{:?}", time_last_message_viewed);
        return ErrorReturn::Object(ProtestApp::format_message_chains_t(
            self_to_message_chain,
            vec_to_self_message_chain,
            time_last_message_viewed,
        ));
    }

    // command: delete message that matches index. otherwise delete last message
    async fn delete_private_message_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_to_alias = args
            .get_one::<String>("agent_to_alias")
            .unwrap()
            .to_string();
        let message_index = args.get_one::<String>("message_index");
        let message_index: Option<u32> =
            message_index.map(|s| s.parse::<u32>().ok()).flatten();
        match ProtestApp::delete_private_message(agent_to_alias, message_index, context)
            .await
        {
            ErrorReturn::Object(_) => match message_index {
                Some(message_index) => {
                    return Ok(Some(String::from(format!(
                        "Success: Message {} Deleted",
                        message_index
                    ))))
                }
                None => {
                    return Ok(Some(String::from(format!(
                        "Success: Last Message Deleted",
                    ))))
                }
            },
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: delete message that matches index. otherwise delete last message
    async fn delete_private_message(
        agent_to_alias: String,
        message_index: Option<u32>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: get agent_to_alias message chain
        // step 0: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // step 1: get agent_self to agent_to messages
        let data_id = String::from(format!(
            "private_messages/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias,
            agent_to_alias
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let mut self_to_message_chain: PrivateMessageChain;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(private_messages_object)) => {
                context.client.end_transaction().await;
                let private_messages =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                self_to_message_chain = private_messages;
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "Client Error: There Are No Messages From {} To {}",
                    agent.alias, agent_to_alias
                )));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: delete appropriate message
        match message_index {
            Some(message_index) => {
                let message_index: usize = message_index as usize;
                if message_index < self_to_message_chain.message_chain.len() {
                    self_to_message_chain.message_chain[message_index].message =
                        String::from(format!("<deleted by {}>", agent.alias))
                }
            }
            None => {
                if self_to_message_chain.message_chain.len() > 0 {
                    let len = self_to_message_chain.message_chain.len();
                    self_to_message_chain.message_chain[len - 1].message =
                        String::from(format!("<deleted by {}>", agent.alias))
                }
            }
        }
        // step 3: commit change to key value store
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let json_private_messages =
            serde_json::to_string(&self_to_message_chain).unwrap();
        match context
            .client
            .set_data(
                data_id.clone(),
                String::from(format!("private_messages")),
                json_private_messages,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
    }

    // PART 4: PUBLIC MESSAGING FUNCTIONALITY

    // command: send public message to all team members. message associated with team
    async fn send_public_message_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let message = args.get_one::<String>("message").unwrap().to_string();
        let message_type = args.get_one::<String>("message_type");
        let message_type = match message_type {
            Some(message_type_str) => message_type_str.to_string(),
            None => String::from(""),
        };
        let message_type = ProtestApp::str_to_message_type(message_type);
        match ProtestApp::send_public_message(message, message_type, context).await {
            ErrorReturn::Object(time) => {
                return Ok(Some(String::from(format!(
                    "Success: Message Sent To Team At Time {}",
                    time
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: send public message to all team members. message associated with team
    async fn send_public_message(
        message: String,
        message_type: MessageType,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<OffsetDateTime> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2.0: prepare a message to append
        let new_message = Message {
            message,
            time_stamp: ProtestApp::get_time().await,
            message_type,
        };
        let time_stamp = new_message.time_stamp.clone();
        // step 2: does public messages vector exist? if not, then create & share one
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let data_id = String::from(format!(
            "public_messages/{}/{}",
            agent.coordinator_alias.unwrap(),
            agent.alias,
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let mut public_messages: PublicMessageChain;
        match context.client.get_data(&data_id.clone()).await {
            // step 2.1: in this case, append to existing message chain
            Ok(Some(private_messages_object)) => {
                public_messages =
                    serde_json::from_str(private_messages_object.data_val()).unwrap();
                // make a just in case check: new message time stamp doesn't violate
                // message chain's time stamp (i.e. it's strictly greater)
                if !(new_message.time_stamp > public_messages.last_message_time_stamp) {
                    return ErrorReturn::Error(String::from("System Error: New Message's Timestamp Violates Time Invariant of Message Chain"));
                }
                public_messages.message_chain.push(new_message);
            }
            // step 2.2: in this case, create a new message chain
            Ok(None) => {
                public_messages = PublicMessageChain {
                    agent_from_alias: agent.alias.clone(),
                    last_message_time_stamp: new_message.time_stamp.clone(),
                    message_chain: vec![new_message],
                };
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        context.client.end_transaction().await;
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        // step 3: commit message chain to key value store
        let json_public_messages = serde_json::to_string(&public_messages).unwrap();
        match context
            .client
            .set_data(
                data_id.clone(),
                String::from(format!("public_messages")),
                json_public_messages,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Public Messages Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 4: share message chain with every member in the team (excluding self)
        let agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 4.0: add every member in the team as a contact
        let mut all_agent_list = agent_list.follower_list;
        all_agent_list
            .insert(agent_list.coordinator.alias.clone(), agent_list.coordinator);
        for (alias, agentl) in &all_agent_list {
            if *alias == agent.alias {
                continue;
            }
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            match context.client.add_contact(agentl.id.clone()).await {
                Ok(_) => {
                    context.client.end_transaction().await;
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Unable To Add {} As Contact. {}",
                        alias, err
                    )));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // step 4.1: add every member in the team as a reader
        std::thread::sleep(std::time::Duration::from_secs(1));
        let mut readers: Vec<&String> = Vec::new();
        for (alias, agent_l) in &all_agent_list {
            if *alias != agent.alias {
                readers.push(&agent_l.name);
            }
        }
        match context.client.add_do_readers(data_id, readers).await {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Message Chain Could Not Be Shared. {}",
                    err
                )));
            }
        }
        return ErrorReturn::Object(time_stamp);
    }

    // helper: generates a list of all agent aliases from an agent list
    fn get_agent_alias_list_vec(agent_list: &AgentList) -> Vec<String> {
        let mut agent_list_vec = Vec::new();
        agent_list_vec.push(agent_list.coordinator.alias.clone());
        for (alias, _agent) in &agent_list.follower_list {
            agent_list_vec.push(alias.clone())
        }
        return agent_list_vec;
    }

    // command: get public message chain. optionally get last num messages
    async fn get_public_messages_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let num_last_messages = args.get_one::<String>("num_last_messages");
        let num_last_messages: Option<u32> =
            num_last_messages.map(|s| s.parse::<u32>().ok()).flatten();
        match ProtestApp::get_public_messages(num_last_messages, context).await {
            ErrorReturn::Object(result) => {
                return Ok(Some(String::from(format!("{}", result))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: get public message chain. optionally get last num messages
    async fn get_public_messages(
        num_last_messages: Option<u32>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let mut both_nonexistent = true;
        // step 1: get agent_self to agent_to messages
        let data_id = String::from(format!(
            "public_messages/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias,
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let self_to_message_chain: Option<PublicMessageChain>;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(public_messages_object)) => {
                context.client.end_transaction().await;
                let public_messages =
                    serde_json::from_str(public_messages_object.data_val()).unwrap();
                self_to_message_chain = Some(public_messages);
                both_nonexistent = false;
            }
            Ok(None) => {
                context.client.end_transaction().await;
                self_to_message_chain = None
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Public Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2.0: generate a list of agent aliases
        let agent_aliases: Vec<String>;
        match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => {
                agent_aliases = ProtestApp::get_agent_alias_list_vec(&agent_list);
            }
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: get public messages from all other agents
        let mut vec_message_chains: Vec<PublicMessageChain> = Vec::new();
        for agent_a in agent_aliases {
            if agent_a == agent.alias {
                continue;
            }
            let data_id = String::from(format!(
                "public_messages/{}/{}",
                agent.coordinator_alias.clone().unwrap(),
                agent_a
            ));
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            let public_message_chain: Option<PublicMessageChain>;
            match context.client.get_data(&data_id.clone()).await {
                Ok(Some(public_messages_object)) => {
                    context.client.end_transaction().await;
                    let public_messages =
                        serde_json::from_str(public_messages_object.data_val()).unwrap();
                    public_message_chain = Some(public_messages);
                    both_nonexistent = false;
                }
                Ok(None) => {
                    context.client.end_transaction().await;
                    public_message_chain = None
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Public Messages Could Not Be Retrieved. {}",
                        err
                    )));
                }
            }
            match public_message_chain {
                Some(unwrapped_message_chain) => {
                    vec_message_chains.push(unwrapped_message_chain);
                }
                None => {}
            };
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // step 3: return formatted messages
        if both_nonexistent {
            return ErrorReturn::Error(String::from(
                "Client Error: There Are No Public Messages",
            ));
        }
        return ErrorReturn::Object(ProtestApp::format_message_chains(
            self_to_message_chain,
            vec_message_chains,
            num_last_messages,
        ));
    }

    // command: delete message that matches index. otherwise delete last message
    async fn delete_public_message_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let message_index = args.get_one::<String>("message_index");
        let message_index: Option<u32> =
            message_index.map(|s| s.parse::<u32>().ok()).flatten();
        match ProtestApp::delete_public_message(message_index, context).await {
            ErrorReturn::Object(_) => match message_index {
                Some(message_index) => {
                    return Ok(Some(String::from(format!(
                        "Success: Message {} Deleted",
                        message_index
                    ))))
                }
                None => {
                    return Ok(Some(String::from(format!(
                        "Success: Last Message Deleted",
                    ))))
                }
            },
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: delete message that matches index. otherwise delete last message
    async fn delete_public_message(
        message_index: Option<u32>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: get agent_to_alias message chain
        // step 0: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent_object) => agent_object,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // step 1: get agent_self to agent_to messages
        let data_id = String::from(format!(
            "public_messages/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias,
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let mut self_to_message_chain: PublicMessageChain;
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(public_messages_object)) => {
                context.client.end_transaction().await;
                let public_messages =
                    serde_json::from_str(public_messages_object.data_val()).unwrap();
                self_to_message_chain = public_messages;
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "Client Error: There Are No Messages From {}",
                    agent.alias
                )));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Public Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: delete appropriate message
        match message_index {
            Some(message_index) => {
                let message_index: usize = message_index as usize;
                if message_index < self_to_message_chain.message_chain.len() {
                    self_to_message_chain.message_chain[message_index].message =
                        String::from(format!("<deleted by {}>", agent.alias))
                }
            }
            None => {
                if self_to_message_chain.message_chain.len() > 0 {
                    let len = self_to_message_chain.message_chain.len();
                    self_to_message_chain.message_chain[len - 1].message =
                        String::from(format!("<deleted by {}>", agent.alias))
                }
            }
        }
        // step 3: commit change to key value store
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let json_public_messages = serde_json::to_string(&self_to_message_chain).unwrap();
        match context
            .client
            .set_data(
                data_id.clone(),
                String::from(format!("public_messages")),
                json_public_messages,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Public Messages Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
    }

    // PART 5: LOCATION DATABASE FUNCTIONALITY

    // command: update agent's location
    async fn update_agent_location_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let agent_location: AgentLocation;
        let x_location = args.get_one::<String>("longitude").unwrap().to_string();
        if x_location == "auto" {
            agent_location = AgentLocation::Auto;
        } else {
            let y_location = args.get_one::<String>("latitude").unwrap().to_string();
            let x_location = x_location.parse::<f64>();
            let x_location = match x_location {
                Ok(x_location) => x_location,
                Err(err) => {
                    return Ok(Some(String::from(format!(
                        "longitude Must Be A Floating Point Number. {}",
                        err
                    ))))
                }
            };
            let y_location = y_location.parse::<f64>();
            let y_location = match y_location {
                Ok(y_location) => y_location,
                Err(err) => {
                    return Ok(Some(String::from(format!(
                        "latitude Must Be A Floating Point Number. {}",
                        err
                    ))))
                }
            };
            let point = coord! {x: x_location, y: y_location};
            agent_location = AgentLocation::Manual(point);
        }
        match ProtestApp::update_agent_location(agent_location, context).await {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Agent Location Has Been Updated",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: update agent's location
    async fn update_agent_location(
        agent_location: AgentLocation,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: commit location to key value store
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let json_agent_location = serde_json::to_string(&agent_location).unwrap();
        match context
            .client
            .set_data(
                String::from("agent_location"),
                String::from("agent_location"),
                json_agent_location,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Private Messages Info Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
    }

    // command: get agent's location
    async fn get_agent_location_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_location(context).await {
            ErrorReturn::Object(location) => {
                return Ok(Some(String::from(format!(
                    "Success: Agent Location Is: longitude: {}. latitude: {}",
                    location.x, location.y,
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command get agent's location
    async fn get_agent_location(context: &mut Arc<Self>) -> ErrorReturn<Coord> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get location to key value store
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .get_data(&String::from("agent_location"))
            .await
        {
            Ok(Some(agent_location_object)) => {
                context.client.end_transaction().await;
                let agent_location: AgentLocation =
                    serde_json::from_str(agent_location_object.data_val()).unwrap();
                match agent_location {
                    AgentLocation::Auto => {
                        let location = coord! {x: AUTO_LOCATION[0], y: AUTO_LOCATION[1]};
                        return ErrorReturn::Object(location);
                    }
                    AgentLocation::Manual(location) => {
                        return ErrorReturn::Object(location)
                    }
                }
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Location Has Not Been Set By Agent",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Public Messages Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
    }

    // helper: automatically get agent's location database
    async fn get_location_database(
        coordinator_alias: String,
        agent_alias: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<Option<Vec<Location>>> {
        // step 1: check that device exists and start transaction
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: retrieve agent's location database
        let data_id = String::from(format!(
            "location_database/{}/{}",
            coordinator_alias, agent_alias
        ));
        match context.client.get_data(&data_id).await {
            Ok(Some(location_database_obj)) => {
                context.client.end_transaction().await;
                let location_database: Vec<Location> =
                    serde_json::from_str(location_database_obj.data_val()).unwrap();
                std::thread::sleep(std::time::Duration::from_secs(1));
                return ErrorReturn::Object(Some(location_database));
            }
            Ok(None) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(1));
                return ErrorReturn::Object(None);
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Agent Location Database Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
    }

    // helper: automatically set agent's location database
    async fn set_location_database(
        coordinator_alias: String,
        agent_alias: String,
        agent_database: Vec<Location>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: check that device exists and start transaction
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        if !context.exists_device().await {
            context.client.end_transaction().await;
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: set agent's location database
        let json_agent_database = serde_json::to_string(&agent_database).unwrap();
        let data_id = String::from(format!(
            "location_database/{}/{}",
            coordinator_alias, agent_alias
        ));
        match context
            .client
            .set_data(
                String::from(data_id),
                String::from("location_database"),
                json_agent_database,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(1));
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Location Database Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
    }

    // helper: share an object with every member in the team (excluding self) as readers
    async fn share_to_team_as_readers(
        data_id: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: get some information from the key value store first
        let agent_alias = match ProtestApp::get_agent_alias(context).await {
            ErrorReturn::Object(agent_alias) => agent_alias,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        let agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 1: add every member in the team as a contact
        let mut all_agent_list = agent_list.follower_list;
        all_agent_list
            .insert(agent_list.coordinator.alias.clone(), agent_list.coordinator);
        for (alias, agent) in &all_agent_list {
            if *alias == agent_alias {
                continue;
            }
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            match context.client.add_contact(agent.id.clone()).await {
                Ok(_) => {
                    context.client.end_transaction().await;
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Unable To Add {} As Contact. {}",
                        alias, err
                    )));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // step 2: add every member in the team as a reader
        let mut readers: Vec<&String> = Vec::new();
        for (alias, agent) in &all_agent_list {
            if *alias != agent_alias {
                readers.push(&agent.name);
            }
        }
        match context.client.add_do_readers(data_id, readers).await {
            Ok(_) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(1));
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Message Chain Could Not Be Shared. {}",
                    err
                )));
            }
        }
    }

    // command: add location to personal database & share with team
    async fn add_to_location_database_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // step 1: extract the latitude and the longitude
        let x_location = args.get_one::<String>("longitude").unwrap().to_string();
        let y_location = args.get_one::<String>("latitude").unwrap().to_string();
        let x_location = x_location.parse::<f64>();
        let x_location = match x_location {
            Ok(x_location) => x_location,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: longitude Must Be A Floating Point Number. {}",
                    err
                ))))
            }
        };
        let y_location = y_location.parse::<f64>();
        let y_location = match y_location {
            Ok(y_location) => y_location,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: latitude Must Be A Floating Point Number. {}",
                    err
                ))))
            }
        };
        let point = coord! {x: x_location, y: y_location};
        // step 2: extract the location type
        let location_type = args.get_one::<String>("location_type").unwrap().to_string();
        let location_type_enum: LocationType;
        if location_type == String::from("Resource") {
            location_type_enum = LocationType::Resource
        } else if location_type == String::from("Danger") {
            location_type_enum = LocationType::Danger
        } else {
            return Ok(Some(String::from(format!(
                "Client Error: location_type Must Be Valid",
            ))));
        }
        // step 3: extract the location information
        let info = args.get_one::<String>("info").unwrap().to_string();
        // step 4: call the update function
        match ProtestApp::add_to_location_database(
            point,
            location_type_enum,
            info,
            context,
        )
        .await
        {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Location Has Been Added To Database",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: add location to personal database & share with team
    async fn add_to_location_database(
        point: Coord,
        location_type: LocationType,
        info: String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        let location = Location {
            point,
            location_type,
            info,
        };
        if agent.coordinator_alias == None {
            return ErrorReturn::Error(String::from(
                "Client Error: Join A Team First Before Attempting To Add Location",
            ));
        }
        // step 1: get agent's location database from key value store and add new location
        let mut location_database = match ProtestApp::get_location_database(
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            context,
        )
        .await
        {
            ErrorReturn::Object(some_location_database) => match some_location_database {
                Some(location_database) => location_database,
                None => Vec::new(),
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        location_database.push(location);
        // step 2: commit new database to key value store
        match ProtestApp::set_location_database(
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            location_database,
            context,
        )
        .await
        {
            ErrorReturn::Object(_) => {}
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        // step 3: share database with all team members
        let data_id = String::from(format!(
            "location_database/{}/{}",
            agent.coordinator_alias.unwrap().clone(),
            agent.alias
        ));
        match ProtestApp::share_to_team_as_readers(data_id, context).await {
            ErrorReturn::Object(_) => return ErrorReturn::Object(String::from("")),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // command: remove location from personal database
    async fn remove_from_location_database_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // step 1: extract the index
        let index = args.get_one::<String>("index").unwrap().to_string();
        let index = index.parse::<usize>();
        let index = match index {
            Ok(index) => index,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: index Must Be A Valid Index Number. {}",
                    err
                ))))
            }
        };
        // step 4: call the remove function
        match ProtestApp::remove_from_location_database(index, context).await {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Location Has Been Removed From Database",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: remove location from personal database
    async fn remove_from_location_database(
        index: usize,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        if agent.coordinator_alias == None {
            return ErrorReturn::Error(String::from(
                "Client Error: Join A Team First Before Attempting To Remove Location",
            ));
        }
        // step 1: get agent's location database from key value store and remove at index
        let mut location_database = match ProtestApp::get_location_database(
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            context,
        )
        .await
        {
            ErrorReturn::Object(some_location_database) => match some_location_database {
                Some(location_database) => location_database,
                None => Vec::new(),
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        if index < location_database.len() {
            location_database.remove(index);
        } else {
            return ErrorReturn::Error(String::from(format!(
                "Client Error: Index {} Is Invalid",
                index
            )));
        }
        // step 2: commit new database to key value store
        match ProtestApp::set_location_database(
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            location_database,
            context,
        )
        .await
        {
            ErrorReturn::Object(_) => {}
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        // step 3: share database with all team members
        let data_id = String::from(format!(
            "location_database/{}/{}",
            agent.coordinator_alias.unwrap().clone(),
            agent.alias
        ));
        match ProtestApp::share_to_team_as_readers(data_id, context).await {
            ErrorReturn::Object(_) => return ErrorReturn::Object(String::from("")),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // helper: format own agent location database
    fn format_own_location_database(
        agent_alias: String,
        location_database: Vec<Location>,
    ) -> String {
        let mut formatted_database = String::from("");
        formatted_database.push_str(&String::from(format!("{}:\n\n", agent_alias)));
        let mut index: u32 = 0;
        for location in location_database {
            formatted_database.push_str(&String::from(format!(
                "{}: {}: longitude: {}, latitude: {}\n",
                index, location.location_type, location.point.x, location.point.y
            )));
            formatted_database.push_str(&String::from(format!("{}\n\n", location.info)));
            index += 1;
        }
        if formatted_database.len() >= 2 {
            formatted_database.truncate(formatted_database.len() - 2);
        }
        return formatted_database;
    }

    // command: get own location database
    async fn get_own_location_database_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_own_location_database(context).await {
            ErrorReturn::Object(location_database) => {
                return Ok(Some(String::from(format!("{}", location_database))))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: get own location database
    async fn get_own_location_database(context: &mut Arc<Self>) -> ErrorReturn<String> {
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        if agent.coordinator_alias == None {
            return ErrorReturn::Error(String::from(
                "Client Error: Agent Must Have Joined A Team First",
            ));
        }
        match ProtestApp::get_location_database(
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            context,
        )
        .await
        {
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
            ErrorReturn::Object(some_location_database) => match some_location_database {
                None => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Own Location Database Does Not Exist",
                    ))
                }
                Some(location_database) => {
                    return ErrorReturn::Object(ProtestApp::format_own_location_database(
                        agent.alias.clone(),
                        location_database,
                    ))
                }
            },
        }
    }

    // helper: format own agent location database
    fn format_location_databases(
        location_databases: Vec<(String, Vec<Location>)>,
    ) -> String {
        let mut formatted_databases = String::from("");
        for location_database in location_databases {
            let mut formatted_database = String::from("");
            formatted_database
                .push_str(&String::from(format!("{}:\n\n", location_database.0)));
            for location in location_database.1 {
                formatted_database.push_str(&String::from(format!(
                    "{}: longitude: {}, latitude: {}\n",
                    location.location_type, location.point.x, location.point.y
                )));
                formatted_database
                    .push_str(&String::from(format!("{}\n\n", location.info)));
            }
            formatted_databases.push_str(&formatted_database);
        }
        if formatted_databases.len() >= 2 {
            formatted_databases.truncate(formatted_databases.len() - 2);
        }
        return formatted_databases;
    }

    // helper: get the haversine distance between 2 coordinates
    fn haversine_distance(coord1: Coord, coord2: Coord) -> f64 {
        let lat1 = coord1.y.to_radians();
        let lon1 = coord1.x.to_radians();
        let lat2 = coord2.y.to_radians();
        let lon2 = coord2.x.to_radians();
        let dlon = lon2 - lon1;
        let dlat = lat2 - lat1;
        let a = (dlat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        // radius of the earth in meters
        let radius = 6371.0 * 1000.0;
        return radius * c;
    }

    // helper: filter location database (location type and distance from)
    async fn filter_location_database(
        mut location_database: Vec<Location>,
        location_type: Option<LocationType>,
        distance_from_agent: Option<f64>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<Vec<Location>> {
        // step 1: filter by location type
        match location_type {
            Some(location_type) => {
                location_database = location_database
                    .into_iter()
                    .filter(|location| location.location_type == location_type)
                    .collect();
            }
            None => {}
        }
        // step 2: filter by distance from agent
        let agent_location = match ProtestApp::get_agent_location(context).await {
            ErrorReturn::Object(point) => point,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        match distance_from_agent {
            Some(distance_from_agent) => {
                location_database = location_database
                    .into_iter()
                    .filter(|location| {
                        ProtestApp::haversine_distance(location.point, agent_location)
                            <= distance_from_agent
                    })
                    .collect();
            }
            None => {}
        }
        return ErrorReturn::Object(location_database);
    }

    // command: get locations from database (with location type and distance from filters)
    async fn get_location_database_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let location_type: Option<LocationType>;
        let location_type_arg = args.get_one::<String>("location_type").unwrap();
        if location_type_arg == "All" {
            location_type = None;
        } else if location_type_arg == "Danger" {
            location_type = Some(LocationType::Danger);
        } else if location_type_arg == "Resource" {
            location_type = Some(LocationType::Resource);
        } else {
            return Ok(Some(String::from(
                "Client Error: Location Type Is Not Valid",
            )));
        }
        let distance_from_agent: Option<f64>;
        let distance_from_agent_arg = args.get_one::<String>("distance_from_agent");
        match distance_from_agent_arg {
            None => distance_from_agent = None,
            Some(distance_from_agent_arg) => {
                let distance_from_agent_arg = distance_from_agent_arg.parse::<f64>();
                match distance_from_agent_arg {
                    Ok(distance_from_agent_arg) => {
                        distance_from_agent = Some(distance_from_agent_arg)
                    }
                    Err(err) => {
                        return Ok(Some(String::from(format!(
                        "Client Error: distance_from_agent Must Be A Valid Integer. {}",
                        err
                    ))))
                    }
                }
            }
        }
        match ProtestApp::get_location_database_real(
            location_type,
            distance_from_agent,
            context,
        )
        .await
        {
            ErrorReturn::Object(location_database) => {
                return Ok(Some(String::from(format!("{}", location_database))))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: get locations from database (with location type and distance filters)
    async fn get_location_database_real(
        location_type: Option<LocationType>,
        distance_from_agent: Option<f64>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: obtain agent alias list
        let agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        let mut complete_agent_list = agent_list.follower_list.clone();
        complete_agent_list.insert(
            agent_list.coordinator.alias.clone(),
            agent_list.coordinator.clone(),
        );
        // step 2: compile a vector of filtered location databases
        let mut filtered_location_databases = Vec::new();
        for (agent_alias, _) in complete_agent_list {
            match ProtestApp::get_location_database(
                agent_list.coordinator.alias.clone(),
                agent_alias.clone(),
                context,
            )
            .await
            {
                ErrorReturn::Object(option_location_database) => {
                    match option_location_database {
                        Some(mut location_database) => {
                            location_database =
                                match ProtestApp::filter_location_database(
                                    location_database,
                                    location_type.clone(),
                                    distance_from_agent,
                                    context,
                                )
                                .await
                                {
                                    ErrorReturn::Object(location_database) => {
                                        location_database
                                    }
                                    ErrorReturn::Error(err) => {
                                        return ErrorReturn::Error(err)
                                    }
                                };
                            filtered_location_databases
                                .push((agent_alias, location_database))
                        }
                        None => {}
                    }
                }
                ErrorReturn::Error(err) => return ErrorReturn::Error(err),
            }
        }
        return ErrorReturn::Object(ProtestApp::format_location_databases(
            filtered_location_databases,
        ));
    }

    // PART 6: OPERATION COMMIT FUNCTIONALITY

    // helper: it's kinda annoying that I have to do this
    fn number_to_month(number: u32) -> Option<Month> {
        match number {
            1 => Some(Month::January),
            2 => Some(Month::February),
            3 => Some(Month::March),
            4 => Some(Month::April),
            5 => Some(Month::May),
            6 => Some(Month::June),
            7 => Some(Month::July),
            8 => Some(Month::August),
            9 => Some(Month::September),
            10 => Some(Month::October),
            11 => Some(Month::November),
            12 => Some(Month::December),
            _ => None,
        }
    }

    // command: set an operation proposal
    async fn make_operation_proposal_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        // step 1: retrieve the operation name & info
        let operation_name = args.get_one::<String>("operation_name").unwrap();
        let info = args.get_one::<String>("info").unwrap();
        // step 2: retrieve the operation start time
        let start_month_arg = args.get_one::<String>("start_month").unwrap();
        let start_month = match start_month_arg.parse::<u32>() {
            Ok(start_month) => start_month,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start_month Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let start_day_arg = args.get_one::<String>("start_day").unwrap();
        let start_day = match start_day_arg.parse::<u8>() {
            Ok(start_day) => start_day,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start_day Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let start_year_arg = args.get_one::<String>("start_year").unwrap();
        let start_year = match start_year_arg.parse::<i32>() {
            Ok(start_year) => start_year,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start_year Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let start_hour_arg = args.get_one::<String>("start_hour").unwrap();
        let start_hour = match start_hour_arg.parse::<u8>() {
            Ok(start_hour) => start_hour,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start_hour Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let start_month = match ProtestApp::number_to_month(start_month) {
            Some(start_month) => start_month,
            None => {
                return Ok(Some(String::from(format!(
                    "Client Error: start_month Must Be A Valid Month",
                ))))
            }
        };
        let start_date = Date::from_calendar_date(start_year, start_month, start_day);
        let start_date = match start_date {
            Ok(start_date) => start_date,
            Err(_) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start Arguments Must Be Valid",
                ))))
            }
        };
        let start_time = Time::from_hms(start_hour, 0, 0);
        let start_time = match start_time {
            Ok(start_time) => start_time,
            Err(_) => {
                return Ok(Some(String::from(format!(
                    "Client Error: start Arguments Must Be Valid",
                ))))
            }
        };
        let start_date = OffsetDateTime::new_utc(start_date, start_time);
        // step 3: retrieve the operation end time
        let end_month_arg = args.get_one::<String>("end_month").unwrap();
        let end_month = match end_month_arg.parse::<u32>() {
            Ok(end_month) => end_month,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end_month Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let end_month = match ProtestApp::number_to_month(end_month) {
            Some(end_month) => end_month,
            None => {
                return Ok(Some(String::from(format!(
                    "Client Error: end_month Must Be A Valid Month",
                ))))
            }
        };
        let end_day_arg = args.get_one::<String>("end_day").unwrap();
        let end_day = match end_day_arg.parse::<u8>() {
            Ok(end_day) => end_day,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end_day Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let end_year_arg = args.get_one::<String>("end_year").unwrap();
        let end_year = match end_year_arg.parse::<i32>() {
            Ok(end_year) => end_year,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end_year Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let end_hour_arg = args.get_one::<String>("end_hour").unwrap();
        let end_hour = match end_hour_arg.parse::<u8>() {
            Ok(end_hour) => end_hour,
            Err(err) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end_hour Must Be A Valid Number. {}",
                    err
                ))))
            }
        };
        let end_date = Date::from_calendar_date(end_year, end_month, end_day);
        let end_date = match end_date {
            Ok(end_date) => end_date,
            Err(_) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end Arguments Must Be Valid",
                ))))
            }
        };
        let end_time = Time::from_hms(end_hour, 0, 0);
        let end_time = match end_time {
            Ok(end_time) => end_time,
            Err(_) => {
                return Ok(Some(String::from(format!(
                    "Client Error: end Arguments Must Be Valid",
                ))))
            }
        };
        let end_date = OffsetDateTime::new_utc(end_date, end_time);
        // start of invariant check: start date <= end_date
        if !(start_date <= end_date) {
            return Ok(Some(String::from(format!(
                "Client Error: end_date Must Be Monotonically Greater Than start_date",
            ))));
        }
        // end of invariant check
        match ProtestApp::make_operation_proposal(
            operation_name,
            start_date,
            end_date,
            info,
            context,
        )
        .await
        {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Operation Proposal Has Been Created",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: set an operation proposal
    async fn make_operation_proposal(
        operation_name: &String,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        info: &String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        // check that agent has joined a team already
        match agent.coordinator_alias {
            Some(_) => {}
            None => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent Has Not Joined A Team Yet",
                ));
            }
        }
        // step a: set operation proposal and share with team
        let data_id = String::from(format!(
            "operation_proposal/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent.alias.clone(),
            operation_name.clone()
        ));
        let operation_proposal = OperationProposal {
            operation_name: operation_name.clone(),
            proposer: agent.alias.clone(),
            start_time,
            end_time,
            info: info.to_string(),
        };
        let json_operation_proposal = serde_json::to_string(&operation_proposal).unwrap();
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from(data_id.clone()),
                String::from("operation_proposal"),
                json_operation_proposal,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Operation Proposal Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
        match ProtestApp::share_to_team_as_readers(data_id, context).await {
            ErrorReturn::Object(_) => return ErrorReturn::Object(String::from("")),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // command: vote for an agent's proposal
    async fn vote_for_operation_proposal_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let operation_name = args.get_one::<String>("operation_name").unwrap();
        let agent_alias = args.get_one::<String>("agent_alias").unwrap();
        let vote = args.get_one::<String>("vote").unwrap();
        let vote_enum: Option<Vote>;
        if vote == "Yes" {
            vote_enum = Some(Vote::Yes);
        } else if vote == "No" {
            vote_enum = Some(Vote::No);
        } else if vote == "N/A" {
            vote_enum = None;
        } else {
            return Ok(Some(String::from(
                "Client Error: Vote Must Be Yes, No, N/A",
            )));
        }
        match ProtestApp::vote_for_operation_proposal(
            operation_name,
            agent_alias,
            vote_enum,
            context,
        )
        .await
        {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Operation Proposal Vote Has Been Cast",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: vote for an agent's proposal
    async fn vote_for_operation_proposal(
        operation_name: &String,
        agent_alias: &String,
        vote: Option<Vote>,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        // check that agent has joined a team already
        match agent.coordinator_alias {
            Some(_) => {}
            None => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent Has Not Joined A Team Yet",
                ));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step a: set operation proposal and share with team
        let data_id = String::from(format!(
            "operation_proposal_vote/{}/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent_alias.clone(),
            operation_name.clone(),
            agent.alias.clone()
        ));
        let json_operation_proposal_vote = serde_json::to_string(&vote).unwrap();
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from(data_id.clone()),
                String::from("operation_proposal_vote"),
                json_operation_proposal_vote,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Operation Proposal Vote Could Not Be Cast. {}",
                    err.to_string()
                )));
            }
        }
        match ProtestApp::share_to_team_as_readers(data_id, context).await {
            ErrorReturn::Object(_) => return ErrorReturn::Object(String::from("")),
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
    }

    // helper: compose vote status information into a string
    fn operation_proposal_status_to_string(
        operation_proposal: OperationProposal,
        agent_votes: HashMap<String, Option<Vote>>,
        committed: bool,
    ) -> String {
        let mut status_string = String::from("");
        // step 1: include operation proposal information
        status_string.push_str(&String::from(format!("committed: {}\n\n", committed)));
        status_string.push_str(&String::from(format!(
            "operation_name: {}\nproposer: {}\nstart_time: {}\nend_time: {}\ninfo: {}\n\n",
            &operation_proposal.operation_name,
            &operation_proposal.proposer,
            ProtestApp::format_offset_date_time(operation_proposal.start_time),
            ProtestApp::format_offset_date_time(operation_proposal.end_time),
            &operation_proposal.info
        )));
        // step 2: include individual agent vote information
        let mut num_agent: u32 = 0;
        let mut num_yes: u32 = 0;
        let mut num_no: u32 = 0;
        let mut num_na: u32 = 0;
        let mut vote_string: String;
        for (agent_alias, vote) in agent_votes {
            match vote {
                Some(vote) => match vote {
                    Vote::Yes => {
                        num_yes += 1;
                        vote_string = String::from("Yes");
                    }
                    Vote::No => {
                        num_no += 1;
                        vote_string = String::from("No");
                    }
                },
                None => {
                    num_na += 1;
                    vote_string = String::from("N/A");
                }
            }
            num_agent += 1;
            status_string
                .push_str(&String::from(format!("{}: {}\n", agent_alias, vote_string)))
        }
        status_string.push_str(&String::from("\n"));
        // step 3: include accumulated agent vote information
        status_string.push_str(&String::from(format!(
            "num_agents: {}\nnum_yes: {}\nnum_no: {}\nnum_n/a {}",
            num_agent, num_yes, num_no, num_na,
        )));
        return status_string;
    }

    // command: see the vote status of an agent's proposal
    async fn operation_proposal_status_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let operation_name = args.get_one::<String>("operation_name").unwrap();
        let agent_alias = args.get_one::<String>("agent_alias").unwrap();
        match ProtestApp::operation_proposal_status(operation_name, agent_alias, context)
            .await
        {
            ErrorReturn::Object(status) => {
                return Ok(Some(String::from(format!("{}", status))))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: see the vote status of an agent's proposal
    async fn operation_proposal_status(
        operation_name: &String,
        agent_alias: &String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        // check that agent has joined a team already
        match agent.coordinator_alias {
            Some(_) => {}
            None => {
                return ErrorReturn::Error(String::from(
                    "Client Error: Agent Has Not Joined A Team Yet",
                ));
            }
        }
        // step 1: retrieve operation proposal
        let operation_proposal: OperationProposal;
        let data_id = String::from(format!(
            "operation_proposal/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent_alias.clone(),
            operation_name.clone()
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(operation_proposal_object)) => {
                context.client.end_transaction().await;
                operation_proposal =
                    serde_json::from_str(operation_proposal_object.data_val()).unwrap();
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Queried Operation Proposal Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Operation Proposal Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 2: compile hash table of agent votes for proposal
        let mut agent_list = match ProtestApp::get_agent_list(context).await {
            ErrorReturn::Object(agent_list) => agent_list,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        agent_list.follower_list.insert(
            agent_list.coordinator.alias.clone(),
            agent_list.coordinator.clone(),
        );
        let mut agent_votes: HashMap<String, Option<Vote>> = HashMap::new();
        for (agent_alias_l, _) in agent_list.follower_list {
            let operation_proposal_vote: Option<Vote>;
            let data_id = String::from(format!(
                "operation_proposal_vote/{}/{}/{}/{}",
                agent.coordinator_alias.clone().unwrap(),
                agent_alias.clone(),
                operation_name.clone(),
                agent_alias_l.clone()
            ));
            let result = context.client.start_transaction();
            if result.is_err() {
                return ErrorReturn::Error(String::from(
                    "System Error: Unable To Start Transaction",
                ));
            }
            match context.client.get_data(&data_id.clone()).await {
                Ok(Some(operation_proposal_vote_object)) => {
                    context.client.end_transaction().await;
                    operation_proposal_vote =
                        serde_json::from_str(operation_proposal_vote_object.data_val())
                            .unwrap();
                }
                Ok(None) => {
                    context.client.end_transaction().await;
                    operation_proposal_vote = None;
                }
                Err(err) => {
                    context.client.end_transaction().await;
                    return ErrorReturn::Error(String::from(format!(
                        "System Error: Operation Proposal Vote Could Not Be Retrieved. {}",
                        err
                    )));
                }
            }
            agent_votes.insert(agent_alias_l, operation_proposal_vote);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // step 3: get committed flag
        let data_id = String::from(format!(
            "operation_proposal/{}/{}/{}/committed",
            agent.coordinator_alias.clone().unwrap(),
            agent_alias.clone(),
            operation_name.clone()
        ));
        let mut committed = false;
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(_)) => {
                context.client.end_transaction().await;
                committed = true;
            }
            Ok(None) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Flag Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        return ErrorReturn::Object(String::from(format!(
            "{}",
            ProtestApp::operation_proposal_status_to_string(
                operation_proposal,
                agent_votes,
                committed
            )
        )));
    }

    // command: commit operation proposal
    async fn commit_operation_proposal_cmd(
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        let operation_name = args.get_one::<String>("operation_name").unwrap();
        let agent_alias = args.get_one::<String>("agent_alias").unwrap();
        match ProtestApp::commit_operation_proposal(operation_name, agent_alias, context)
            .await
        {
            ErrorReturn::Object(_) => {
                return Ok(Some(String::from(
                    "Success: Operation Proposal Has Been Committed",
                )))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: commit operation proposal
    async fn commit_operation_proposal(
        operation_name: &String,
        agent_alias: &String,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 0: housekeeping tasks
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        let agent = match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => agent,
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        };
        match agent.role {
            Role::Coordinator => {}
            Role::Follower => {
                return ErrorReturn::Error(String::from(
                    "Client Error: This Is a Coordinator Only Operation",
                ))
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 1: get committed operations list
        let mut committed_operations_list: Vec<OperationProposal>;
        let data_id = String::from("committed_operations_list");
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(committed_operations_list_object)) => {
                context.client.end_transaction().await;
                committed_operations_list =
                    serde_json::from_str(committed_operations_list_object.data_val())
                        .unwrap();
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Committed Operations List Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Operations List Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // step 1.5: get operation to commit
        let operation_proposal: OperationProposal;
        let data_id = String::from(format!(
            "operation_proposal/{}/{}/{}",
            agent.coordinator_alias.clone().unwrap(),
            agent_alias.clone(),
            operation_name.clone()
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(operation_proposal_object)) => {
                context.client.end_transaction().await;
                operation_proposal =
                    serde_json::from_str(operation_proposal_object.data_val()).unwrap();
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Queried Operation Proposal Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Operation Proposal Could Not Be Retrieved. {}",
                    err
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        if !(operation_proposal.start_time <= operation_proposal.end_time) {
            return ErrorReturn::Error(String::from(
                "Client Error: end_date Must Be Monotonically Greater Than start_date",
            ));
        }
        // step 2: insert operation if possible
        let mut insertion_found = false;
        let mut break_point = 0;
        if committed_operations_list.len() == 0 {
            insertion_found = true;
        } else {
            for i in 0..=committed_operations_list.len() {
                if i == 0 {
                    if operation_proposal.end_time
                        <= committed_operations_list[i].start_time
                    {
                        break_point = i;
                        insertion_found = true;
                        break;
                    }
                } else if i == committed_operations_list.len() {
                    if operation_proposal.start_time
                        >= committed_operations_list[i - 1].end_time
                    {
                        break_point = i;
                        insertion_found = true;
                        break;
                    }
                } else {
                    if operation_proposal.start_time
                        >= committed_operations_list[i - 1].end_time
                        && operation_proposal.end_time
                            <= committed_operations_list[i].start_time
                    {
                        break_point = i;
                        insertion_found = true;
                        break;
                    }
                }
            }
        }
        if insertion_found {
            committed_operations_list.insert(break_point, operation_proposal);
        } else {
            return ErrorReturn::Error(String::from("Client Error: Operation Proposal's Start And End Times Are Incompatible With Already Committed Operations"));
        }
        // step 3: commit new operations list to key value store
        let json_committed_operations_list =
            serde_json::to_string(&committed_operations_list).unwrap();
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                String::from("committed_operations_list"),
                String::from("committed_operations_list"),
                json_committed_operations_list,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Operations List Could Not Be Updated. {}",
                    err.to_string()
                )));
            }
        }
        // step 4: add committed flag to key value store
        let committed = true;
        let json_committed = serde_json::to_string(&committed).unwrap();
        let data_id = String::from(format!(
            "operation_proposal/{}/{}/{}/committed",
            agent.coordinator_alias.clone().unwrap(),
            agent_alias.clone(),
            operation_name.clone()
        ));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context
            .client
            .set_data(
                data_id,
                String::from("committed_operation/committed"),
                json_committed,
                None,
                None,
                false,
            )
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return ErrorReturn::Object(String::from(""));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Flag Could Not Be Added. {}",
                    err.to_string()
                )));
            }
        }
    }

    // command: get committed operation proposals list
    async fn get_committed_operations_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_committed_operations(context).await {
            ErrorReturn::Object(committed_operations) => {
                return Ok(Some(String::from(format!("{}", committed_operations))))
            }
            ErrorReturn::Error(err) => return Ok(Some(String::from(format!("{}", err)))),
        }
    }

    // not command: get committed operation proposals list
    async fn get_committed_operations(context: &mut Arc<Self>) -> ErrorReturn<String> {
        let committed_operations_list: Vec<OperationProposal>;
        let data_id = String::from("committed_operations_list");
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        match context.client.get_data(&data_id.clone()).await {
            Ok(Some(committed_operations_list_object)) => {
                context.client.end_transaction().await;
                committed_operations_list =
                    serde_json::from_str(committed_operations_list_object.data_val())
                        .unwrap();
            }
            Ok(None) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(
                    "Client Error: Committed Operations List Does Not Exist",
                ));
            }
            Err(err) => {
                context.client.end_transaction().await;
                return ErrorReturn::Error(String::from(format!(
                    "System Error: Committed Operations List Could Not Be Retrieved. {}",
                    err
                )));
            }
        };
        // format committed operations list into a string
        let mut committed_operations_str = String::from("");
        for operation in committed_operations_list {
            committed_operations_str.push_str(&String::from(format!(
                "operation_name: {}\nproposer: {}\nstart_time: {}\nend_time: {}\ninfo: {}\n\n",
                &operation.operation_name,
                &operation.proposer,
                ProtestApp::format_offset_date_time(operation.start_time),
                ProtestApp::format_offset_date_time(operation.end_time),
                &operation.info
            )));
        }
        if committed_operations_str.len() >= 2 {
            committed_operations_str.truncate(committed_operations_str.len() - 2);
        }
        return ErrorReturn::Object(committed_operations_str);
    }
}

// spin up client application & command line interface
#[tokio::main]
async fn main() -> ReplResult<()> {
    let app = Arc::new(ProtestApp::new().await);
    let mut repl = Repl::new(app.clone())
        .with_name("Protest App")
        .with_version("v0.1.0")
        .with_description("Protest App")
        .with_command_async(Command::new("ccc").about("ccc"), |_, context| {
            Box::pin(ProtestApp::create_coordinator_cmd(context))
        })
        .with_command_async(Command::new("ab").about("ab"), |_, context| {
            Box::pin(ProtestApp::accept_alice_bob_cmd(context))
        })
        .with_command_async(Command::new("abc").about("abc"), |_, context| {
            Box::pin(ProtestApp::accept_alice_bob_charles_cmd(context))
        })
        .with_command_async(
            Command::new("ca")
                .arg(Arg::new("coordinator_id").required(true))
                .arg(Arg::new("coordinator_name").required(true))
                .about("ca <coordinator_id> <coordinator_name>"),
            |args, context| Box::pin(ProtestApp::create_alice_cmd(args, context)),
        )
        .with_command_async(
            Command::new("cb")
                .arg(Arg::new("coordinator_id").required(true))
                .arg(Arg::new("coordinator_name").required(true))
                .about("cb <coordinator_id> <coordinator_name>"),
            |args, context| Box::pin(ProtestApp::create_bob_cmd(args, context)),
        )
        .with_command_async(
            Command::new("cc")
                .arg(Arg::new("coordinator_id").required(true))
                .arg(Arg::new("coordinator_name").required(true))
                .about("cc <coordinator_id> <coordinator_name>"),
            |args, context| Box::pin(ProtestApp::create_carol_cmd(args, context)),
        )
        .with_command_async(
            Command::new("ch").about("check_join_team_request"),
            |_, context| Box::pin(ProtestApp::check_join_team_request_cmd(context)),
        )
        .with_command_async(
            Command::new("dump_data").about("dump_data"),
            |_, context| Box::pin(ProtestApp::dump_data_cmd(context)),
        )
        .with_command_async(
            Command::new("signup_agent")
                .arg(Arg::new("alias").required(true))
                .about("signup_agent <alias>"),
            |args, context| Box::pin(ProtestApp::signup_agent_cmd(args, context)),
        )
        .with_command_async(
            Command::new("login_agent")
                .arg(Arg::new("id").required(true))
                .about("login_agent <id>"),
            |args, context| Box::pin(ProtestApp::login_agent_cmd(args, context)),
        )
        .with_command_async(
            Command::new("get_agent_info").about("get_agent_info"),
            |_, context| Box::pin(ProtestApp::get_agent_info_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_alias").about("get_agent_alias"),
            |_, context| Box::pin(ProtestApp::get_agent_alias_cmd(context)),
        )
        .with_command_async(
            Command::new("update_agent_alias")
                .arg(Arg::new("alias").required(true))
                .about("update_agent_alias <alias>"),
            |args, context| Box::pin(ProtestApp::update_agent_alias_cmd(args, context)),
        )
        .with_command_async(
            Command::new("get_agent_id").about("get_agent_id"),
            |_, context| Box::pin(ProtestApp::get_agent_id_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_name").about("get_agent_name"),
            |_, context| Box::pin(ProtestApp::get_agent_name_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_coordinator_id").about("get_agent_coordinator_id"),
            |_, context| Box::pin(ProtestApp::get_agent_coordinator_alias_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_role").about("get_agent_role"),
            |_, context| Box::pin(ProtestApp::get_agent_role_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_alias_list").about("get_agent_alias_list"),
            |_, context| Box::pin(ProtestApp::get_agent_alias_list_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_list").about("get_agent_list"),
            |_, context| Box::pin(ProtestApp::get_agent_list_cmd(context)),
        )
        .with_command_async(
            Command::new("create_team").about("create_team"),
            |_, context| Box::pin(ProtestApp::create_team_cmd(context)),
        )
        .with_command_async(
            Command::new("join_team_request")
                .arg(Arg::new("coordinator_id").required(true))
                .arg(Arg::new("coordinator_name").required(true))
                .about("join_team_request <coordinator_id> <coordinator_name>"),
            |args, context| Box::pin(ProtestApp::join_team_request_cmd(args, context)),
        )
        .with_command_async(
            Command::new("join_team_accept_request")
                .arg(Arg::new("agent_alias").required(true))
                .about("join_team_accept_request <agent_alias>"),
            |args, context| {
                Box::pin(ProtestApp::join_team_accept_request_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("remove_from_team")
                .arg(Arg::new("agent_alias").required(true))
                .about("remove_from_team <agent_alias>"),
            |args, context| Box::pin(ProtestApp::remove_from_team_cmd(args, context)),
        )
        .with_command_async(
            Command::new("check_join_team_request").about("check_join_team_request"),
            |_, context| Box::pin(ProtestApp::check_join_team_request_cmd(context)),
        )
        .with_command_async(
            Command::new("send_private_message")
                .arg(Arg::new("agent_to_alias").required(true))
                .arg(Arg::new("message").required(true))
                .arg(Arg::new("message_type").required(false))
                .about("send_private_message <message> <message_type>"),
            |args, context| Box::pin(ProtestApp::send_private_message_cmd(args, context)),
        )
        .with_command_async(
            Command::new("get_private_messages")
                .arg(Arg::new("agent_to_alias").required(true))
                .arg(Arg::new("num_last_messages").required(false))
                .about("get_private_messages <agent_to_alias> <num_last_messages>"),
            |args, context| Box::pin(ProtestApp::get_private_messages_cmd(args, context)),
        )
        .with_command_async(
            Command::new("get_new_private_messages")
                .arg(Arg::new("agent_to_alias").required(true))
                .about("get_new_private_messages <agent_to_alias>"),
            |args, context| {
                Box::pin(ProtestApp::get_new_private_messages_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("delete_private_message")
                .arg(Arg::new("agent_to_alias").required(true))
                .arg(Arg::new("message_index").required(false))
                .about("delete_private_message <agent_to_alias> <message_index>"),
            |args, context| {
                Box::pin(ProtestApp::delete_private_message_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("send_public_message")
                .arg(Arg::new("message").required(true))
                .arg(Arg::new("message_type").required(false))
                .about("send_public_message <message> <message_type>"),
            |args, context| Box::pin(ProtestApp::send_public_message_cmd(args, context)),
        )
        .with_command_async(
            Command::new("get_public_messages")
                .arg(Arg::new("num_last_messages").required(false))
                .about("get_public_messages <num_last_messages>"),
            |args, context| Box::pin(ProtestApp::get_public_messages_cmd(args, context)),
        )
        .with_command_async(
            Command::new("delete_public_message")
                .arg(Arg::new("agent_to_alias").required(true))
                .arg(Arg::new("message_index").required(false))
                .about("delete_public_message <agent_to_alias> <message_index>"),
            |args, context| {
                Box::pin(ProtestApp::delete_public_message_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("update_agent_location")
                .arg(Arg::new("longitude").required(true))
                .arg(Arg::new("latitude").required(false))
                .about("update_agent_location <longitude> <latitude> or delete_public_message auto"),
            |args, context| {
                Box::pin(ProtestApp::update_agent_location_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("get_agent_location")
                .about("update_agent_location"),
            |_, context| {
                Box::pin(ProtestApp::get_agent_location_cmd(context))
            },
        )
        .with_command_async(
            Command::new("add_to_location_database")
                .arg(Arg::new("longitude").required(true))
                .arg(Arg::new("latitude").required(true))
                .arg(Arg::new("location_type").required(true))
                .arg(Arg::new("info").required(true))
                .about("add_to_location_database <longitude> <latitude> <location_type> <info>\n
                location_type: Danger, Resource"),
            |args, context| {
                Box::pin(ProtestApp::add_to_location_database_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("remove_from_location_database")
                .arg(Arg::new("index").required(true))
                .about("remove_from_location_database <index>"),
            |args, context| {
                Box::pin(ProtestApp::remove_from_location_database_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("get_own_location_database")
                .about("get_own_location_database <index>"),
            |_, context| {
                Box::pin(ProtestApp::get_own_location_database_cmd(context))
            },
        )
        .with_command_async(
            Command::new("get_location_database")
                .arg(Arg::new("location_type").required(true))
                .arg(Arg::new("distance_from_agent").required(false))
                .about("get_own_location_database <location_type> <distance_from_agent (in km)>\n
                location_type: All, Danger, Resource"),
            |args, context| {
                Box::pin(ProtestApp::get_location_database_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("make_operation_proposal")
                .arg(Arg::new("operation_name").required(true))
                .arg(Arg::new("start_month").required(true))
                .arg(Arg::new("start_day").required(true))
                .arg(Arg::new("start_year").required(true))
                .arg(Arg::new("start_hour").required(true))
                .arg(Arg::new("end_month").required(true))
                .arg(Arg::new("end_day").required(true))
                .arg(Arg::new("end_year").required(true))
                .arg(Arg::new("end_hour").required(true))
                .arg(Arg::new("info").required(true))
                .about("make_operation_proposal <operation_name> <start_month> <start_day> <start_year> <start_hour>
                <end_month> <end_day> <end year> <end_hour> <info>"),
            |args, context| {
                Box::pin(ProtestApp::make_operation_proposal_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("vote_for_operation_proposal")
                .arg(Arg::new("agent_alias").required(true))
                .arg(Arg::new("operation_name").required(true))
                .arg(Arg::new("vote").required(true))
                .about("vote_for_operation_proposal <agent_alias> <operation_name> <vote: Yes, No, N/A>"),
            |args, context| {
                Box::pin(ProtestApp::vote_for_operation_proposal_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("operation_proposal_status")
                .arg(Arg::new("agent_alias").required(true))
                .arg(Arg::new("operation_name").required(true))
                .about("operation_proposal_status <agent_alias> <operation_name>"),
            |args, context| {
                Box::pin(ProtestApp::operation_proposal_status_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("commit_operation_proposal")
                .arg(Arg::new("agent_alias").required(true))
                .arg(Arg::new("operation_name").required(true))
                .about("commit_operation_proposal <agent_alias> <operation_name>"),
            |args, context| {
                Box::pin(ProtestApp::commit_operation_proposal_cmd(args, context))
            },
        )
        .with_command_async(
            Command::new("get_committed_operations")
                .about("get_committed_operations"),
            |_, context| {
                Box::pin(ProtestApp::get_committed_operations_cmd(context))
            },
        );
    repl.run_async().await
}
