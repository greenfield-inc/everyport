# CLI reference

Every `ppm` command and its options, exactly as `ppm <command> --help` prints them. For what each one is for, see the [README](../README.md#cli).

## `ppm`

```text
See every dev server running on your machine. Run with no command for the terminal UI.

Usage: ppm [OPTIONS] [COMMAND]

Commands:
  list     List servers once
  watch    Print a snapshot on every change
  stop     Stop the server on a port
  restart  Stop it, then rerun its command in the folder it started from
  open     Open the server in your browser, forwarding its port with --on
  clean    Stop the servers Clean up suggests
  stdio    Speak the ppm protocol on stdin and stdout
  serve    Speak the ppm protocol over HTTP on loopback
  remote   Manage remote machines
  doctor   Check permissions and platform support
  help     Print this message or the help of the given subcommand(s)

Options:
      --on <MACHINE>  Run the command on another machine
  -y, --yes           Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help          Print help
  -V, --version       Print version
```

## `ppm list`

```text
List servers once

Usage: ppm list [OPTIONS]

Options:
      --json  Print the snapshot as JSON
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm watch`

```text
Print a snapshot on every change

Usage: ppm watch [OPTIONS] --jsonl

Options:
      --jsonl  One JSON snapshot per line
  -y, --yes    Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help   Print help
```

## `ppm stop`

```text
Stop the server on a port

Usage: ppm stop [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
      --force      Kill instead of asking it to quit, even if it's protected
      --protected  Stop it even if it's protected, asking it to quit first
  -y, --yes        Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help       Print help
```

## `ppm restart`

```text
Stop it, then rerun its command in the folder it started from

Usage: ppm restart [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
      --protected  Restart it even if it's protected
  -y, --yes        Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help       Print help
```

## `ppm open`

```text
Open the server in your browser, forwarding its port with --on

Usage: ppm open [OPTIONS] <PORT>

Arguments:
  <PORT>  

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm clean`

```text
Stop the servers Clean up suggests

Usage: ppm clean [OPTIONS]

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm stdio`

```text
Speak the ppm protocol on stdin and stdout

Usage: ppm stdio [OPTIONS]

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm serve`

```text
Speak the ppm protocol over HTTP on loopback

Usage: ppm serve [OPTIONS]

Options:
      --listen <LISTEN>        Loopback address to listen on [default: 127.0.0.1:7767]
      --url <URL>              URL clients use to reach this server, such as a Tailscale URL, for the connection code
  -y, --yes                    Don't ask: install ppm on the machine, or stop what clean suggests
      --allow-origin <ORIGIN>  Web origin whose pages may use the server, such as https://dash.example.com (repeatable)
  -h, --help                   Print help
```

## `ppm remote`

```text
Manage remote machines

Usage: ppm remote [OPTIONS] <COMMAND>

Commands:
  add   Save a machine: `ppm remote add devbox -- ssh devbox`, or `--code` from `ppm serve`
  list  List saved and discovered machines, and the ppm installed on each
  rm    Remove a saved machine
  help  Print this message or the help of the given subcommand(s)

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm remote add`

```text
Save a machine: `ppm remote add devbox -- ssh devbox`, or `--code` from `ppm serve`

Usage: ppm remote add [OPTIONS] <NAME> [-- <COMMAND>...]

Arguments:
  <NAME>        Name to use with --on
  [COMMAND]...  Command prefix that runs a program on the machine

Options:
      --code <CODE>  Connection code that `ppm serve` prints
  -y, --yes          Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help         Print help
```

## `ppm remote list`

```text
List saved and discovered machines, and the ppm installed on each

Usage: ppm remote list [OPTIONS]

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm remote rm`

```text
Remove a saved machine

Usage: ppm remote rm [OPTIONS] <NAME>

Arguments:
  <NAME>  

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```

## `ppm doctor`

```text
Check permissions and platform support

Usage: ppm doctor [OPTIONS]

Options:
  -y, --yes   Don't ask: install ppm on the machine, or stop what clean suggests
  -h, --help  Print help
```
