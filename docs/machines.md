# Connect a machine

Everyport reaches an SSH machine in three steps. WSL distros, Docker containers and `everyport serve` machines skip the first two.

1. **Turn on SSH on the machine.** On a Mac: System Settings > General > Sharing > Remote Login. On Linux: `sudo systemctl enable --now ssh` (`sshd` on Fedora and Arch). On Windows, in PowerShell as administrator:
   ```powershell
   Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Start-Service sshd; Set-Service sshd -StartupType Automatic
   ```
2. **Let the machine accept your key.** Make one with `ssh-keygen` if you don't have one, then run `ssh-copy-id you@machine`. From Windows, which has no `ssh-copy-id`, run `type $env:USERPROFILE\.ssh\id_ed25519.pub | ssh you@machine "cat >> .ssh/authorized_keys"`. On a Windows machine, the key goes in `C:\Users\<you>\.ssh\authorized_keys`, or in `C:\ProgramData\ssh\administrators_authorized_keys` if you're an administrator there.
3. **Add it.** Open **Settings → Machines** and click **Add**, or click the machine in the popover, or run `everyport --on <machine>`. Everyport asks, then installs itself there.

Everyport lists machines it finds under **Found on this computer**: hosts in `~/.ssh/config`, your [Pane](https://runpane.com) remote hosts, online Tailscale peers when the `tailscale` CLI is installed, and WSL distros on Windows. They show with a grey ring and don't connect until you click them.

## When a machine can't connect

The popover shows each step and the fix under the one that failed. After fixing it, click **Check again**. **Check** beside a machine in **Settings → Machines** shows the same steps. In the terminal:

```text
$ everyport doctor --on studio-mac
studio-mac (ssh studio-mac.tail1234.ts.net)
✓ studio-mac.tail1234.ts.net resolves to 100.64.0.7
✗ Nothing listens on port 22
  SSH is off on studio-mac. On studio-mac, turn on System Settings > General > Sharing > Remote Login.
    Connection refused (os error 61)
```

`everyport doctor` without `--on` checks this computer, then every saved and found machine.

| Step | Fix |
|---|---|
| The name resolves | Check the name, and that you're on the same network, VPN or tailnet. |
| The SSH port answers | Refused: turn on SSH (step 1). No answer: wake the machine, or join its network. |
| ssh signs in with your key | Add your key (step 2). For a new or changed host key, run `ssh` to the machine once in a terminal and accept it. |
| The machine's OS is known | Everyport has builds for macOS, Linux and Windows on x86_64 and ARM64. The details show what the machine answered. |
| `everyport` is installed | Add the machine (step 3), and Everyport installs itself. |

After a failure, Everyport tries again after 1 s, doubling up to 60 s. It tries right away when you click the machine or your network changes. When ssh rejects your key or the host key, it waits for you to click **Check again**.

Pane reaches its remote hosts over its own connection, so a machine can work in Pane with SSH off. Turn on SSH for Everyport.
