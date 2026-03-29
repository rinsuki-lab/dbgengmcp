use std::sync::{Arc, Mutex};

use rmcp::{
    ErrorData,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::ErrorCode,
    schemars::JsonSchema,
    serde::Deserialize,
};

use crate::windbg::DebuggerClient;

#[derive(Clone)]
pub struct DebuggerService {
    state: Arc<Mutex<DebuggerServiceState>>,
    tool_router: ToolRouter<Self>,
}

struct DebuggerServiceState {
    client: Option<DebuggerClient>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConnectRequest {
    #[schemars(
        description = "The connection string for WinDbg, e.g. tcp:Port=12345,Server=localhost"
    )]
    remote: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExecuteCommandRequest {
    #[schemars(description = "The command to execute in WinDbg")]
    command: String,
}

#[rmcp::tool_router]
impl DebuggerService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DebuggerServiceState { client: None })),
            tool_router: Self::tool_router(),
        }
    }

    #[rmcp::tool(description = "Connect to specificed WinDbg instance")]
    pub async fn connect(
        &self,
        Parameters(params): Parameters<ConnectRequest>,
    ) -> Result<String, ErrorData> {
        let client = DebuggerClient::new(params.remote).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to connect: {:?}", e),
                None,
            )
        })?;
        self.state.lock().unwrap().client = Some(client);
        Ok("Connected".to_string())
    }

    #[rmcp::tool(description = "Execute command in connected WinDbg instance")]
    pub async fn execute_command(
        &self,
        Parameters(params): Parameters<ExecuteCommandRequest>,
    ) -> Result<String, ErrorData> {
        let client = {
            let state = self.state.lock().unwrap();
            match &state.client {
                Some(c) => c.clone(),
                None => {
                    return Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        "Not connected to any WinDbg instance. Please call connect first."
                            .to_string(),
                        None,
                    ));
                }
            }
        };
        Ok(client.execute_command(params.command).await.unwrap())
    }

    #[rmcp::tool(description = "Break the currently debugging program in connected WinDbg instance")]
    pub async fn break_program(&self) -> Result<String, ErrorData> {
        let client = {
            let state = self.state.lock().unwrap();
            match &state.client {
                Some(c) => c.clone(),
                None => {
                    return Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        "Not connected to any WinDbg instance. Please call connect first."
                            .to_string(),
                        None,
                    ));
                }
            }
        };
        client.break_program().await.unwrap();
        Ok("Program interrupted".to_string())
    }

    #[rmcp::tool(description = "Disconnect from WinDbg instance")]
    pub async fn disconnect(&self) -> Result<String, ErrorData> {
        let mut state = self.state.lock().unwrap();
        let client = match state.client.take() {
            Some(c) => c,
            None => {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    "Currently not connected to any WinDbg instance.".to_string(),
                    None,
                ));
            }
        };
        client.close();
        Ok("Disconnected".to_string())
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for DebuggerService {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
}
