# Bot Wrapper System

This directory contains a wrapper system for the Meri Bot Rust that enables proper restart functionality.

## Files

- `bot_wrapper.ps1` - PowerShell wrapper script that manages bot processes
- `forcerestart.bat` - Batch file called by the bot's `^forcerestart` command
- `WRAPPER_README.md` - This documentation file

## How It Works

### 1. Starting the Bot

Instead of running `cargo run` directly, use the wrapper:

```powershell
.\bot_wrapper.ps1 start
```

Or simply:
```powershell
.\bot_wrapper.ps1
```

### 2. The `^forcerestart` Command

When you use the `^forcerestart` command in Discord (bot owner only):

1. The bot saves all conversation contexts to disk
2. The bot calls `forcerestart.bat`
3. The batch file calls `bot_wrapper.ps1 forcerestart`
4. The PowerShell wrapper:
   - Finds all bot processes (cargo and meri_bot_rust)
   - Sends Ctrl+C twice to each process
   - Force kills any remaining processes
   - Waits for cleanup
   - Runs `cargo build` to ensure latest code
   - Runs `cargo run` to start the bot again

### 3. Manual Commands

You can also use the wrapper manually:

```powershell
# Start the bot
.\bot_wrapper.ps1 start

# Stop the bot gracefully
.\bot_wrapper.ps1 stop

# Restart the bot
.\bot_wrapper.ps1 restart

# Force restart (same as restart)
.\bot_wrapper.ps1 forcerestart
```

## Process Management

The wrapper ensures proper process cleanup by:

1. **Finding Processes**: Looks for both `cargo` processes running the bot and `meri_bot_rust` executables
2. **Graceful Shutdown**: Sends Ctrl+C (SIGINT) twice to each process
3. **Force Kill**: If processes don't respond to Ctrl+C, force kills them
4. **Cleanup Wait**: Waits 2 seconds to ensure all processes are stopped
5. **Double Check**: Verifies no bot processes remain before starting

## Requirements

- Windows PowerShell (included with Windows 10/11)
- Rust and Cargo installed
- Bot configuration files in place

## Troubleshooting

### If the bot doesn't restart properly:

1. Check that `bot_wrapper.ps1` and `forcerestart.bat` are in the same directory as `Cargo.toml`
2. Ensure PowerShell execution policy allows running scripts:
   ```powershell
   Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
   ```
3. Check that the bot owner ID is correctly set in `botconfig.txt`

### Manual restart if needed:

```powershell
# Stop all bot processes
.\bot_wrapper.ps1 stop

# Start the bot
.\bot_wrapper.ps1 start
```

## Security Note

The `^forcerestart` command can only be used by the bot owner (configured via `BOT_OWNER_ID` in `botconfig.txt`). This prevents unauthorized restarts.

## File Locations

Make sure these files are in your bot's root directory (same directory as `Cargo.toml`):
- `bot_wrapper.ps1`
- `forcerestart.bat`
- `Cargo.toml`
- `botconfig.txt` 