# RS-ADB-MCP

Rust로 작성된 ADB(Android Debug Bridge) MCP(Model Context Protocol) 서버입니다.

## 기능

이 MCP 서버는 다음과 같은 ADB 도구를 제공합니다:

| 도구 이름 | 설명 |
|-----------|------|
| `adb_devices` | 연결된 Android 기기 및 에뮬레이터 목록 조회 |
| `adb_shell` | Android 기기에서 셸 명령 실행 |
| `adb_install` | APK 파일 설치 |
| `adb_logcat` | 로그캡 로그 조회 |
| `adb_pull` | 기기에서 파일 가져오기 |
| `adb_push` | 기기로 파일 복사 |
| `adb_activity_manager` | Activity Manager(am) 명령 실행 |
| `adb_package_manager` | Package Manager(pm) 명령 실행 |
| `adb_inspect_ui` | UI 계층 구조(XML) 캡처 |
| `adb_screenshot` | 화면 스크린샷 캡처 |

## 요구사항

- Rust 1.70+
- ADB (Android Debug Bridge)가 설치되어 있어야 함
- ADB가 PATH에 등록되어 있어야 함

## 설치

### 소스에서 빌드

```bash
cd RS-adb-mcp
cargo build --release
```

빌드된 바이너리는 `target/release/rs-adb-mcp.exe`에 생성됩니다.

## Cline 설정

Cline에서 이 MCP 서버를 사용하려면, VS Code 설정에 다음을 추가하세요:

```json
{
  "mcpServers": {
    "rs-adb-mcp": {
      "command": "c:\\\\__GIT\\\\RS-adb-mcp\\\\target\\\\release\\\\rs-adb-mcp.exe",
      "args": [],
      "env": {},
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

경로는 실제 빌드 위치에 맞게 수정하세요.

## 사용 예시

### 기기 목록 조회

```json
{
  "name": "adb_devices",
  "arguments": {}
}
```

### 셸 명령 실행

```json
{
  "name": "adb_shell",
  "arguments": {
    "command": "ls -la /sdcard"
  }
}
```

### APK 설치

```json
{
  "name": "adb_install",
  "arguments": {
    "apk_path": "C:\\\\path\\\\to\\\\app.apk"
  }
}
```

### 로그캡 조회

```json
{
  "name": "adb_logcat",
  "arguments": {
    "lines": 100,
    "filter": "ActivityManager"
  }
}
```

### 파일 가져오기

```json
{
  "name": "adb_pull",
  "arguments": {
    "remote_path": "/sdcard/screenshot.png"
  }
}
```

### 파일 복사 (Base64)

```json
{
  "name": "adb_push",
  "arguments": {
    "file_base64": "base64encodedcontent...",
    "remote_path": "/sdcard/file.txt"
  }
}
```

### 화면 캡처

```json
{
  "name": "adb_screenshot",
  "arguments": {
    "as_base64": true
  }
}
```

### UI 계층 구조 캡처

```json
{
  "name": "adb_inspect_ui",
  "arguments": {}
}
```

### Activity Manager 사용

```json
{
  "name": "adb_activity_manager",
  "arguments": {
    "am_command": "start",
    "am_args": "-a android.intent.action.VIEW -d http://example.com"
  }
}
```

### Package Manager 사용

```json
{
  "name": "adb_package_manager",
  "arguments": {
    "pm_command": "list",
    "pm_args": "packages -3"
  }
}
```

## 라이선스

MIT
