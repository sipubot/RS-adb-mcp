use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

// ============================================================================
// MCP 프로토콜 모델
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRpcSuccessResponse {
    jsonrpc: String,
    id: Value,
    result: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRpcErrorResponse {
    jsonrpc: String,
    id: Value,
    error: JsonRpcError,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcSuccessResponse {
    fn new(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result,
        }
    }
}

impl JsonRpcErrorResponse {
    fn new(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            error: JsonRpcError {
                code,
                message,
                data: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum JsonRpcResponse {
    Success(JsonRpcSuccessResponse),
    Error(JsonRpcErrorResponse),
}

// ============================================================================
// MCP 프로토콜 타입
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerCapabilities {
    tools: Option<ToolsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolsCapability {
    list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    protocol_version: String,
    capabilities: ServerCapabilities,
    server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListToolsResult {
    tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TextContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallResult {
    content: Vec<TextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

// ============================================================================
// ADB MCP 서버
// ============================================================================

struct AdbMcpServer;

impl AdbMcpServer {
    fn new() -> Self {
        Self
    }

    async fn run_adb(&self, args: Vec<&str>, device: Option<&str>) -> Result<String> {
        let mut cmd = Command::new("adb");

        if let Some(d) = device {
            cmd.arg("-s").arg(d);
        }

        for arg in args {
            cmd.arg(arg);
        }

        debug!("Running ADB command: {:?}", cmd);

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute ADB command")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            anyhow::bail!("ADB command failed: {}", stderr);
        }

        if !stderr.is_empty() {
            warn!("ADB stderr: {}", stderr);
        }

        Ok(stdout.to_string())
    }

    async fn run_adb_shell_command(&self, device: Option<&str>, command: &str) -> Result<String> {
        self.run_adb(vec!["shell", command], device).await
    }

    fn get_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "adb_devices".to_string(),
                description: "Lists all connected Android devices and emulators with their status and details".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "adb_shell".to_string(),
                description: "Executes a shell command on a connected Android device or emulator".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
            Tool {
                name: "adb_install".to_string(),
                description: "Installs an Android application (APK) on a connected device or emulator".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "apk_path": {
                            "type": "string",
                            "description": "Local path to APK file"
                        }
                    },
                    "required": ["apk_path"]
                }),
            },
            Tool {
                name: "adb_logcat".to_string(),
                description: "Retrieves Android system and application logs from a connected device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "filter": {
                            "type": "string",
                            "description": "Logcat filter expression (optional)"
                        },
                        "lines": {
                            "type": "integer",
                            "description": "Number of lines to return (default: 50)"
                        }
                    },
                    "required": []
                }),
            },
            Tool {
                name: "adb_pull".to_string(),
                description: "Transfers a file from a connected Android device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "remote_path": {
                            "type": "string",
                            "description": "Remote file path on the device"
                        },
                        "as_base64": {
                            "type": "boolean",
                            "description": "Return as base64 (default: true)"
                        }
                    },
                    "required": ["remote_path"]
                }),
            },
            Tool {
                name: "adb_push".to_string(),
                description: "Transfers a file from the server to a connected Android device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "file_base64": {
                            "type": "string",
                            "description": "Base64 encoded file content"
                        },
                        "remote_path": {
                            "type": "string",
                            "description": "Remote file path on the device"
                        }
                    },
                    "required": ["file_base64", "remote_path"]
                }),
            },
            Tool {
                name: "adb_activity_manager".to_string(),
                description: "Executes Activity Manager (am) commands on a connected Android device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "am_command": {
                            "type": "string",
                            "description": "Activity Manager subcommand (e.g., 'start', 'broadcast', 'force-stop')"
                        },
                        "am_args": {
                            "type": "string",
                            "description": "Arguments for the am subcommand"
                        }
                    },
                    "required": ["am_command"]
                }),
            },
            Tool {
                name: "adb_package_manager".to_string(),
                description: "Executes Package Manager (pm) commands on a connected Android device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "pm_command": {
                            "type": "string",
                            "description": "Package Manager subcommand (e.g., 'list', 'grant', 'revoke')"
                        },
                        "pm_args": {
                            "type": "string",
                            "description": "Arguments for the pm subcommand"
                        }
                    },
                    "required": ["pm_command"]
                }),
            },
            Tool {
                name: "adb_inspect_ui".to_string(),
                description: "Captures the complete UI hierarchy of the current screen as XML".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "as_base64": {
                            "type": "boolean",
                            "description": "Return XML as base64 (default: false)"
                        }
                    },
                    "required": []
                }),
            },
            Tool {
                name: "adb_screenshot".to_string(),
                description: "Captures the current screen of a connected Android device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device ID (optional)"
                        },
                        "as_base64": {
                            "type": "boolean",
                            "description": "Return as base64 (default: false)"
                        }
                    },
                    "required": []
                }),
            },
        ]
    }

    async fn handle_initialize(&self, id: Value) -> JsonRpcResponse {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
            },
            server_info: ServerInfo {
                name: "rs-adb-mcp".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        JsonRpcResponse::Success(JsonRpcSuccessResponse::new(
            id,
            serde_json::to_value(result).unwrap(),
        ))
    }

    async fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        let result = ListToolsResult {
            tools: self.get_tools(),
        };

        JsonRpcResponse::Success(JsonRpcSuccessResponse::new(
            id,
            serde_json::to_value(result).unwrap(),
        ))
    }

    async fn handle_tools_call(&self, id: Value, params: Value) -> JsonRpcResponse {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

        let result = match self.execute_tool(name, arguments).await {
            Ok(content) => ToolCallResult {
                content,
                is_error: Some(false),
            },
            Err(e) => ToolCallResult {
                content: vec![TextContent {
                    content_type: "text".to_string(),
                    text: format!("Error: {}", e),
                }],
                is_error: Some(true),
            },
        };

        JsonRpcResponse::Success(JsonRpcSuccessResponse::new(
            id,
            serde_json::to_value(result).unwrap(),
        ))
    }

    async fn execute_tool(&self, name: &str, arguments: Value) -> Result<Vec<TextContent>> {
        match name {
            "adb_devices" => self.adb_devices(arguments).await,
            "adb_shell" => self.adb_shell(arguments).await,
            "adb_install" => self.adb_install(arguments).await,
            "adb_logcat" => self.adb_logcat(arguments).await,
            "adb_pull" => self.adb_pull(arguments).await,
            "adb_push" => self.adb_push(arguments).await,
            "adb_activity_manager" => self.adb_activity_manager(arguments).await,
            "adb_package_manager" => self.adb_package_manager(arguments).await,
            "adb_inspect_ui" => self.adb_inspect_ui(arguments).await,
            "adb_screenshot" => self.adb_screenshot(arguments).await,
            _ => anyhow::bail!("Unknown tool: {}", name),
        }
    }

    // =========================================================================
    // ADB 도구 구현
    // =========================================================================

    async fn adb_devices(&self, _arguments: Value) -> Result<Vec<TextContent>> {
        let output = self.run_adb(vec!["devices", "-l"], None).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_shell(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .context("Missing 'command' argument")?;

        let output = self.run_adb_shell_command(device, command).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_install(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let apk_path = arguments
            .get("apk_path")
            .and_then(|v| v.as_str())
            .context("Missing 'apk_path' argument")?;

        let path = std::path::Path::new(apk_path);
        if !path.exists() {
            anyhow::bail!("APK file not found: {}", apk_path);
        }

        let output = self.run_adb(vec!["install", apk_path], device).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_logcat(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let filter = arguments.get("filter").and_then(|v| v.as_str());
        let lines = arguments.get("lines").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let lines_str = lines.to_string();
        let mut args = vec!["logcat", "-d", "-t", &lines_str];
        if let Some(f) = filter {
            args.push("-s");
            args.push(f);
        }

        let output = self.run_adb(args, device).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_pull(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let remote_path = arguments
            .get("remote_path")
            .and_then(|v| v.as_str())
            .context("Missing 'remote_path' argument")?;
        let as_base64 = arguments.get("as_base64").and_then(|v| v.as_bool()).unwrap_or(true);

        let temp_dir = std::env::temp_dir();
        let file_name = std::path::Path::new(remote_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("pulled_file");
        let local_path = temp_dir.join(file_name);

        self.run_adb(vec!["pull", remote_path, local_path.to_str().unwrap()], device)
            .await?;

        let content = tokio::fs::read(&local_path).await?;
        let _ = tokio::fs::remove_file(&local_path).await;

        let text = if as_base64 {
            base64::engine::general_purpose::STANDARD.encode(&content)
        } else {
            String::from_utf8(content).context("File is not valid UTF-8 text")?
        };

        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text,
        }])
    }

    async fn adb_push(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let file_base64 = arguments
            .get("file_base64")
            .and_then(|v| v.as_str())
            .context("Missing 'file_base64' argument")?;
        let remote_path = arguments
            .get("remote_path")
            .and_then(|v| v.as_str())
            .context("Missing 'remote_path' argument")?;

        let content = base64::engine::general_purpose::STANDARD.decode(file_base64)?;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("push_temp");
        tokio::fs::write(&temp_file, content).await?;

        let output = self
            .run_adb(vec!["push", temp_file.to_str().unwrap(), remote_path], device)
            .await;

        let _ = tokio::fs::remove_file(&temp_file).await;

        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output?,
        }])
    }

    async fn adb_activity_manager(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let am_command = arguments
            .get("am_command")
            .and_then(|v| v.as_str())
            .context("Missing 'am_command' argument")?;
        let am_args = arguments.get("am_args").and_then(|v| v.as_str());

        let mut shell_cmd = format!("am {}", am_command);
        if let Some(args) = am_args {
            shell_cmd.push(' ');
            shell_cmd.push_str(args);
        }

        let output = self.run_adb_shell_command(device, &shell_cmd).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_package_manager(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let pm_command = arguments
            .get("pm_command")
            .and_then(|v| v.as_str())
            .context("Missing 'pm_command' argument")?;
        let pm_args = arguments.get("pm_args").and_then(|v| v.as_str());

        let mut shell_cmd = format!("pm {}", pm_command);
        if let Some(args) = pm_args {
            shell_cmd.push(' ');
            shell_cmd.push_str(args);
        }

        let output = self.run_adb_shell_command(device, &shell_cmd).await?;
        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text: output,
        }])
    }

    async fn adb_inspect_ui(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let as_base64 = arguments.get("as_base64").and_then(|v| v.as_bool()).unwrap_or(false);

        let output_path = "/sdcard/window_dump.xml";
        let dump_cmd = format!("uiautomator dump {}", output_path);

        self.run_adb_shell_command(device, &dump_cmd).await?;

        let temp_dir = std::env::temp_dir();
        let local_path = temp_dir.join("window_dump.xml");
        self.run_adb(vec!["pull", output_path, local_path.to_str().unwrap()], device)
            .await?;

        let content = tokio::fs::read_to_string(&local_path).await?;
        let _ = tokio::fs::remove_file(&local_path).await;

        let text = if as_base64 {
            base64::engine::general_purpose::STANDARD.encode(content.as_bytes())
        } else {
            content
        };

        Ok(vec![TextContent {
            content_type: "text".to_string(),
            text,
        }])
    }

    async fn adb_screenshot(&self, arguments: Value) -> Result<Vec<TextContent>> {
        let device = arguments.get("device").and_then(|v| v.as_str());
        let as_base64 = arguments.get("as_base64").and_then(|v| v.as_bool()).unwrap_or(false);

        let screenshot_path = "/sdcard/screen.png";
        let screenshot_cmd = format!("screencap -p {}", screenshot_path);

        self.run_adb_shell_command(device, &screenshot_cmd).await?;

        let temp_dir = std::env::temp_dir();
        let local_path = temp_dir.join("screen.png");
        self.run_adb(vec!["pull", screenshot_path, local_path.to_str().unwrap()], device)
            .await?;

        if as_base64 {
            let content = tokio::fs::read(&local_path).await?;
            let _ = tokio::fs::remove_file(&local_path).await;
            let base64_content = base64::engine::general_purpose::STANDARD.encode(&content);
            Ok(vec![TextContent {
                content_type: "text".to_string(),
                text: base64_content,
            }])
        } else {
            let _ = tokio::fs::remove_file(&local_path).await;
            Ok(vec![TextContent {
                content_type: "text".to_string(),
                text: "Screenshot captured successfully".to_string(),
            }])
        }
    }
}

// ============================================================================
// 메인 함수
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting RS-ADB-MCP Server...");

    let server = AdbMcpServer::new();
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        debug!("Received: {}", line);

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                let response = JsonRpcErrorResponse::new(
                    Value::Null,
                    -32700,
                    "Parse error".to_string(),
                );
                println!("{}", serde_json::to_string(&response)?);
                continue;
            }
        };

        let response = match request.method.as_str() {
            "initialize" => server.handle_initialize(request.id).await,
            "tools/list" => server.handle_tools_list(request.id).await,
            "tools/call" => {
                if let Some(params) = request.params {
                    server.handle_tools_call(request.id, params).await
                } else {
                    JsonRpcResponse::Error(JsonRpcErrorResponse::new(
                        request.id,
                        -32602,
                        "Missing params".to_string(),
                    ))
                }
            }
            _ => JsonRpcResponse::Error(JsonRpcErrorResponse::new(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            )),
        };

        let response_json = serde_json::to_string(&response)?;
        println!("{}", response_json);
        std::io::stdout().flush()?;
        debug!("Sent: {}", response_json);
    }

    info!("RS-ADB-MCP Server shutting down...");
    Ok(())
}
