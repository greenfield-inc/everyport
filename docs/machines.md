# Connect a machine

Everyport reaches another machine over SSH, then installs itself there. It needs three things:

1. **SSH is on.** On a Mac: System Settings > General > Sharing > Remote Login. On Linux: `sudo systemctl enable --now ssh`. On Windows, in PowerShell as administrator:
   ```powershell
   Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Start-Service sshd; Set-Service sshd -StartupType Automatic
   ```
2. **The machine accepts your key.** Run `ssh-copy-id you@machine`. For Windows, add your public key to `C:\Users\<you>\.ssh\authorized_keys`, or to `C:\ProgramData\ssh\administrators_authorized_keys` if you're an administrator there.
3. **Pick it.** Choose it in the app or run `everyport --on <machine>`. Everyport checks the OS and CPU and installs the matching `everyport` there, after asking.

Machines that Everyport finds show grey until you pick one. It finds hosts in `~/.ssh/config`, your Pane remote hosts, online Tailscale peers when the `tailscale` CLI is installed, and WSL distros on Windows.

## When a machine can't connect

The app shows each step and the fix under the one that failed. Click **Check again** after fixing it. In the terminal, run the same check:

```text
$ everyport doctor --on studio-mac
studio-mac (ssh studio-mac.tail1234.ts.net)
✓ studio-mac.tail1234.ts.net resolves to 100.64.0.7
✗ Nothing listens on port 22
  SSH is off on studio-mac. On it, turn on System Settings > General > Sharing > Remote Login.
    Connection refused (os error 61)
```

`everyport doctor` without `--on` checks this computer, then every saved and found machine. The steps are:

| Step | When it fails |
|---|---|
| The name resolves | Check the name, and that you're on the same network, VPN or tailnet. |
| Port 22 answers | Refused: SSH is off (see step 1). No answer: the machine is off, asleep or on another network. |
| ssh signs in with your key | Add your key (see step 2). A new or changed host key: run `ssh` to the machine once in a terminal and accept it. |
| The machine's OS | Everyport runs on macOS, Linux and Windows, on x86_64 and ARM64. |
| `everyport` is installed | Pick the machine, and Everyport installs itself. |

Everyport tries a failed machine again after 1, 2, 4 and up to 60 seconds, and right away when you click it or your network changes. When ssh rejects your key or the host key, it waits for you to click **Check again**.

A Pane remote host can be reachable in Pane with SSH off, since Pane uses its own connection.
