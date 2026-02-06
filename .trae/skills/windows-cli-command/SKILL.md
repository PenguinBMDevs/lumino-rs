---
name: "windows-cli-command"
description: "Enforces using windows_cli tools for all command-line operations. Invoke when any command-line execution is needed in Windows environment."
---

# Windows CLI Command Executor

This skill enforces the use of windows_cli tools for all command-line operations in Windows environment.

## When to Invoke

**ALWAYS invoke this skill when:**
- User asks to execute any command-line command
- User needs to run shell commands (PowerShell, CMD, Git Bash)
- User wants to execute terminal operations
- Any task requires command-line execution

## Mandatory Tools

You MUST use the following tools for command execution:

### 1. `mcp_windows-cli_execute_command`
Execute a command in the specified shell (powershell, cmd, or gitbash)

**Parameters:**
- `command` (required): Command to execute
- `shell` (required): Shell type - "powershell", "cmd", or "gitbash"
- `workingDir` (optional): Working directory for command execution

**Example usage:**
```json
{
  "shell": "powershell",
  "command": "Get-Location",
  "workingDir": "C:\\Users\\username"
}
```

### 2. `mcp_windows-cli_get_current_directory`
Get the current working directory

### 3. `mcp_windows-cli_get_command_history`
Get the history of executed commands

### 4. SSH Tools (if needed)
- `mcp_windows-cli_create_ssh_connection`
- `mcp_windows-cli_ssh_execute`
- `mcp_windows-cli_ssh_disconnect`
- `mcp_windows-cli_read_ssh_connections`
- `mcp_windows-cli_update_ssh_connection`
- `mcp_windows-cli_delete_ssh_connection`

## Forbidden Actions

**NEVER use:**
- `RunCommand` tool for Windows command execution
- Direct shell invocation without windows_cli tools
- Any other command execution methods in Windows environment

## Workflow

1. Identify the command to execute
2. Choose appropriate shell type:
   - **powershell**: Recommended for most Windows operations
   - **cmd**: For legacy Windows commands
   - **gitbash**: For Unix-style commands in Windows
3. Use `mcp_windows-cli_execute_command` with proper parameters
4. Check results and handle errors appropriately
5. If command history is needed, use `mcp_windows-cli_get_command_history`

## Examples

### Example 1: List files in directory
```json
{
  "shell": "powershell",
  "command": "Get-ChildItem",
  "workingDir": "C:\\Projects"
}
```

### Example 2: Run npm install
```json
{
  "shell": "powershell",
  "command": "npm install",
  "workingDir": "D:\\source\\my-project"
}
```

### Example 3: Git operations
```json
{
  "shell": "gitbash",
  "command": "git status",
  "workingDir": "/d/source/my-project"
}
```

## Important Notes

- Always inform the user before executing commands
- For long-running processes, consider the impact on system resources
- Handle errors gracefully and provide clear feedback to the user
- Use appropriate shell type based on the command requirements
- Working directory paths should be absolute paths
