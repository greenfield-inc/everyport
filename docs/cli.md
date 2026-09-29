# CLI reference

Every `everyport` command and its options, exactly as `everyport <command> --help` prints them. For what each one is for, see the [README](../README.md#cli).

## `everyport`

```text
See every dev server running on your machine. Run with no command for the terminal UI.

Usage: everyport [OPTIONS] [COMMAND]

Commands:
  list     List servers once
  watch    Print a snapshot on every change
  stop     Stop the server on a port
  restart  Stop it, then rerun its command in the folder it started from
  open     Open the server in your browser, forwarding its port with --on
  clean    Stop the servers Clean up suggests
  stdio    Speak the Everyport protocol on stdin and stdout
  serve    Speak the Everyport protocol over HTTP on loopback
  remote   Manage remote machines
  doctor   Check permissions and platform support
  help     Print this message or the help of the given subcommand(s)

Options:
      --on <MACHINE>  Run the command on another machine
  -y, --yes           Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help          Print help
  -V, --version       Print version
```

## `everyport list`

```text
List servers once

Usage: everyport list [OPTIONS]

Options:
      --json  Print the snapshot as JSON
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport watch`

```text
Print a snapshot on every change

Usage: everyport watch [OPTIONS] --jsonl

Options:
      --jsonl  One JSON snapshot per line
  -y, --yes    Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help   Print help
```

## `everyport stop`

```text
Stop the server on a port

Usage: everyport stop [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
      --force      Kill instead of asking it to quit, even if it's protected
      --protected  Stop it even if it's protected, asking it to quit first
  -y, --yes        Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help       Print help
```

## `everyport restart`

```text
Stop it, then rerun its command in the folder it started from

Usage: everyport restart [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
      --protected  Restart it even if it's protected
  -y, --yes        Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help       Print help
```

## `everyport open`

```text
Open the server in your browser, forwarding its port with --on

Usage: everyport open [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport clean`

```text
Stop the servers Clean up suggests

Usage: everyport clean [OPTIONS]

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport stdio`

```text
Speak the Everyport protocol on stdin and stdout

Usage: everyport stdio [OPTIONS]

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport serve`

```text
Speak the Everyport protocol over HTTP on loopback

Usage: everyport serve [OPTIONS]

Options:
      --listen <LISTEN>        Loopback address to listen on [default: 127.0.0.1:7767]
      --url <URL>              URL clients use to reach this server, such as a Tailscale URL, for the connection code
  -y, --yes                    Don't ask: install everyport on the machine, or stop what clean suggests
      --allow-origin <ORIGIN>  Web origin whose pages may use the server, such as https://dash.example.com (repeatable)
  -h, --help                   Print help
```

## `everyport remote`

```text
Manage remote machines

Usage: everyport remote [OPTIONS] <COMMAND>

Commands:
  add   Save a machine: `everyport remote add devbox -- ssh devbox`, or `--code` from `everyport serve`
  list  List saved and discovered machines, and the everyport installed on each
  rm    Remove a saved machine
  help  Print this message or the help of the given subcommand(s)

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport remote add`

```text
Save a machine: `everyport remote add devbox -- ssh devbox`, or `--code` from `everyport serve`

Usage: everyport remote add [OPTIONS] <NAME> [-- <COMMAND>...]

Arguments:
  <NAME>        Name to use with --on
  [COMMAND]...  Command prefix that runs a program on the machine

Options:
      --code <CODE>  Connection code that `everyport serve` prints
  -y, --yes          Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help         Print help
```

## `everyport remote list`

```text
List saved and discovered machines, and the everyport installed on each

Usage: everyport remote list [OPTIONS]

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport remote rm`

```text
Remove a saved machine

Usage: everyport remote rm [OPTIONS] <NAME>

Arguments:
  <NAME>  

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `everyport doctor`

```text
Check permissions and platform support

Usage: everyport doctor [OPTIONS]

Options:
  -y, --yes   Don't ask: install everyport on the machine, or stop what clean suggests
  -h, --help  Print help
```
