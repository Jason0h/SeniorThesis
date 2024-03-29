// TODO: find a way for scuba internal code to not panic
// TODO: find a way for artificial waits to not be necessary for code functionality

// TODO: figure out why all data is unavailable after login. solved? (N)
// diagnosis ---> internal issue. data is lost if all devices associated with the id are
// closed. temporary hack: make sure one device associated with the id is open at al times
// not quite?.....

// TODO: personalized login message (maybe not until above todo is fixed?)

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
// miscellanious imports
use std::collections::HashMap;
use strum_macros::Display;

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
    coordinator_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct AgentList {
    coordinator: Agent,
    follower_list: HashMap<String, Agent>,
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
    coordinator_id: String,
    status: JoinTeamRequestStatus,
}

// reference: naming standard for objects in scuba key-value store
// "agent": private rw permission
// "agent_list": coordinator write permission. follower read permission
// "join_team_request/{agent_alias}": follower + coordinatior rw permission

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

    // PART 1: MISCELLANIOUS BASIC FUNCTIONALITY

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

    // command: create client device and save personal information
    async fn signup_agent_cmd(
        args: ArgMatches,
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
        // step 3: setup client data in memory
        let agent = Agent {
            id: context.client.idkey(),
            name: context.client.linked_name(),
            alias: args.get_one::<String>("alias").unwrap().to_string(),
            role: Role::Follower,
            coordinator_id: None,
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
                    args.get_one::<String>("alias").unwrap().to_string(),
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
        match context
            .client
            .create_linked_device(args.get_one::<String>("id").unwrap().to_string())
            .await
        {
            Ok(_) => {
                context.client.end_transaction().await;
                return Ok(Some(String::from("Success: Welcome Back!")));
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
                let coordinator_id = match agent.coordinator_id {
                    Some(coordinator_id) => coordinator_id,
                    None => String::from("N/A"),
                };
                return Ok(Some(String::from(format!(
                    "Success: Id: {}, Alias: {}, Name: {}, Role: {}, Coordinator-Id: {}",
                    agent.id, agent.alias, agent.name, agent.role, coordinator_id
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
        match agent.coordinator_id {
            Some(_) => {}
            None => {
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

    // command: return client's coordinator's id
    async fn get_agent_coordinator_id_cmd(
        context: &mut Arc<Self>,
    ) -> ReplResult<Option<String>> {
        match ProtestApp::get_agent_coordinator_id(context).await {
            ErrorReturn::Object(coordinator_id) => {
                return Ok(Some(String::from(format!(
                    "Success: Coordinator's Id Is: {}",
                    coordinator_id
                ))))
            }
            ErrorReturn::Error(err) => return Ok(Some(err)),
        }
    }

    // not command: return client's coordinator's id
    async fn get_agent_coordinator_id(context: &mut Arc<Self>) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 2: get and return client's coordinator's id
        match ProtestApp::get_agent_info(context).await {
            ErrorReturn::Object(agent) => match agent.role {
                Role::Follower => {
                    let coordinator_id = match agent.coordinator_id {
                        Some(coordinator_id) => coordinator_id,
                        None => String::from("N/A"),
                    };
                    ErrorReturn::Object(coordinator_id)
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

    // command: retrieve agent aliases list (must be coordinator or have joined a team)
    async fn get_team_alias_list_cmd(
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
                    "Success: Team Has Been Created. Agents May Send Join Team Requests To Your Id",
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
        std::thread::sleep(std::time::Duration::from_secs(2));
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
        std::thread::sleep(std::time::Duration::from_secs(2));
        // step 5: promote own role to coordinator
        agent.role = Role::Coordinator;
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
            coordinator_id: coordinator_id.clone(),
            status: JoinTeamRequestStatus::Active,
        };
        let json_join_team_request = serde_json::to_string(&join_team_request).unwrap();
        // step 2.1: check that agent has not joined a team
        match agent.coordinator_id {
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

        std::thread::sleep(std::time::Duration::from_secs(2));
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
        std::thread::sleep(std::time::Duration::from_secs(2));
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
        std::thread::sleep(std::time::Duration::from_secs(2));
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
        match ProtestApp::join_team_accept_request(args, context).await {
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
        args: ArgMatches,
        context: &mut Arc<Self>,
    ) -> ErrorReturn<String> {
        // step 1: check that device exists
        if !context.exists_device().await {
            return ErrorReturn::Error(String::from(
                "Client Error: Device Does Not Exist. Please Login First",
            ));
        }
        // step 1.5: check that agent is a Coordinator, not a Follower
        match ProtestApp::get_agent_role(context).await {
            ErrorReturn::Object(role) => match role {
                Role::Follower => {
                    return ErrorReturn::Error(String::from(
                        "Client Error: Followers Cannot Accept Join Team Requests",
                    ))
                }
                Role::Coordinator => {}
            },
            ErrorReturn::Error(err) => return ErrorReturn::Error(err),
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
        // note: keep track of reason (or no reason) to reject join team request
        let mut status = JoinTeamRequestStatus::Accepted;
        // step 2: retrieve agent's join team request (if it exists).
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        let agent_alias = args.get_one::<String>("agent_alias").unwrap().to_string();
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
                std::thread::sleep(std::time::Duration::from_secs(2));
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
        std::thread::sleep(std::time::Duration::from_secs(2));
        let result = context.client.start_transaction();
        if result.is_err() {
            return ErrorReturn::Error(String::from(
                "System Error: Unable To Start Transaction",
            ));
        }
        join_team_request.status = status.clone();
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
                std::thread::sleep(std::time::Duration::from_secs(2));
                let mut agent_list = match ProtestApp::get_agent_list(context).await {
                    ErrorReturn::Object(agent_list) => agent_list,
                    ErrorReturn::Error(err) => return ErrorReturn::Error(err),
                };
                std::thread::sleep(std::time::Duration::from_secs(2));
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
                // step 3.6 share agent list (as a reader) with new agent
                std::thread::sleep(std::time::Duration::from_secs(2));
                let reader = join_team_request.agent.name.clone();
                let readers = vec![&reader];
                match context
                    .client
                    .add_do_readers(String::from("agent_list"), readers)
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
        std::thread::sleep(std::time::Duration::from_secs(2));
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
                        agent.coordinator_id = Some(join_team_request.coordinator_id);
                        std::thread::sleep(std::time::Duration::from_secs(2));
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

    // command

    // not command

    // PART 4: PUBLIC MESSAGING FUNCTIONALITY

    // command

    // not command

    // PART 5: LOCATION DATABASE FUNCTIONALITY

    // command

    // not command

    // PART 6: OPERATION COMMIT FUNCTIONALITY

    // command

    // not command
}

// spin up client application & command line interface
#[tokio::main]
async fn main() -> ReplResult<()> {
    let app = Arc::new(ProtestApp::new().await);
    let mut repl = Repl::new(app.clone())
        .with_name("Protest App")
        .with_version("v0.1.0")
        .with_description("Protest App")
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
            |_, context| Box::pin(ProtestApp::get_agent_coordinator_id_cmd(context)),
        )
        .with_command_async(
            Command::new("get_agent_role").about("get_agent_role"),
            |_, context| Box::pin(ProtestApp::get_agent_role_cmd(context)),
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
            Command::new("check_join_team_request").about("check_join_team_request"),
            |_, context| Box::pin(ProtestApp::check_join_team_request_cmd(context)),
        );
    repl.run_async().await
}
