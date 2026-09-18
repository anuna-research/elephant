---
title: How to run Elephant on login
mode: how-to
---

# How to run Elephant on login

This guide configures a macOS LaunchAgent for an existing Elephant identity.
Complete [[how-to-share-a-theory]] before relying on peer updates.

## Configure the service

Use `daemon run` because launchd supervises the foreground process.
Save the following as `~/Library/LaunchAgents/io.anuna.elephant.plist`.
Replace the example paths with absolute paths for your account.
Launchd does not expand `~` or shell variables inside the file.
For a non-default store, add its absolute `ELEPHANT_HOME` path to `EnvironmentVariables`.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>            <string>io.anuna.elephant</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Users/YOU/.local/bin/elephant</string>
    <string>daemon</string>
    <string>run</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>ELEPHANT_SYNC_INTERVAL</key> <string>30</string>
  </dict>
  <key>RunAtLoad</key>        <true/>
  <key>KeepAlive</key>        <true/>
  <key>StandardOutPath</key>  <string>/Users/YOU/Library/Logs/elephant-daemon.log</string>
  <key>StandardErrorPath</key><string>/Users/YOU/Library/Logs/elephant-daemon.log</string>
</dict>
</plist>
```

## Start and check

If a manually started daemon uses this store, stop it before loading the service.
If `~/Library/Logs` does not exist, create it.
Load the service and inspect its state:

```bash
launchctl bootstrap gui/$UID ~/Library/LaunchAgents/io.anuna.elephant.plist
launchctl print gui/$UID/io.anuna.elephant | head   # verify it is running
elephant daemon status --json
```

Check the reported sync interval.
Use `elephant info --json` to check the selected store.

## Restart or unload

After replacing the binary, restart the service:

```sh
launchctl kickstart -k gui/$UID/io.anuna.elephant
```

To stop the service, unload it:

```sh
launchctl bootout gui/$UID/io.anuna.elephant
```

After changing the plist's environment, unload and bootstrap the service to apply it.
Operational notes:

- When the daemon exits, launchd's `KeepAlive` setting restarts it, including after `elephant daemon stop`.
  Use `launchctl bootout` to stop it permanently until the next bootstrap.
- One store supports one daemon.
  `daemon start` returns an existing live daemon; it does not replace its environment.
  A second foreground daemon cannot acquire the store lock.
- Add a `RUST_LOG` key (e.g. `elephant=debug`) to `EnvironmentVariables`
  for sync-session tracing in the log file.
