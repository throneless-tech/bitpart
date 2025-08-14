# Bitpart 🤖
[![Run checks](https://github.com/throneless-tech/bitpart/actions/workflows/run_checks.yaml/badge.svg)](https://github.com/throneless-tech/bitpart/actions/workflows/run_checks.yaml) 

Bitpart is a messaging tool that runs on top of Signal to support activists, journalists, and human rights defenders.

## Building

This repository contains three components: the Bitpart server, a command-line client for connecting to the Bitpart server, and a storage adapter used for storing Signal key information to Bitpart's database. In order to build all three components, make sure you have a _2024 edition of Rust_ and run:

```
  cargo build
```

## Installing

Visit the [releases page](https://github.com/throneless-tech/bitpart/releases) to download the latest binaries for your operating system and architecture!

## Usage

The Bitpart server expects certain configuration parameters. The following examples use the parameters defined below:

- `<BIND>`: the IP address and port that Bitpart listens on for client connections. For example, `127.0.0.1:3000` would mean that Bitpart is listening on port 3000 on localhost. This is only used by the command-line client and is not necessary to expose to the public internet.
- `<AUTH>`: the token used to authenticate client connections on the above port. The token must match for both the server and the command-line client for them to be able to connect.
- `<DATABASE>`: the path to an SQLite database file where Bitpart stores its state. This file is created if it does not exist.
- `<KEY>`: the encryption key for the SQLite database. Bitpart uses an integrated copy of [SQLCipher](https://www.zetetic.net/sqlcipher/open-source/) to encrypt its database. If Bitpart creates a new database file, it will be initialized with this key. The key must be the same between different runs of Bitpart or otherwise it will not be able to decrypt its database.

### Bare metal

Assuming the `bitpart` binary is in your path, you can view the inline help for the Bitpart server:

```
  bitpart --help
```

Or run it as follows:

```
  bitpart --bind <BIND> --auth <AUTH> --database <DATABASE> --key <KEY>
```

Bitpart can also read configuration parameters from environment variables corresponding to its command-line parameters. For example, you could specify the encryption key via defining the environment variable `BITPART_KEY`.

### Container

Bitpart is available in a Docker-compatible container. For example, to run Bitpart on port 3000 and mounting a database from the current directory:

```
  docker run -d --name bitpart -p 3000:3000 -v ./bitpart.sqlite:/bitpart.sqlite -e BITPART_BIND=127.0.0.1:3000 -e BITPART_DATABASE=/bitpart.sqlite -e BITPART_AUTH=connect to the Bitpart server<AUTH> -e BITPART_KEY=<KEY> ghcr.io/throneless-tech/bitpart:latest
```

### Connecting with the client

Assuming the `bitpart-cli` binary is in your path, you can print out the inline help for the command-line client via:

```
  bitpart-cli --help
```

Or print out the help for a given subcommand via:

```
  bitpart-cli help <SUBCOMMAND>
```

For example, to list available bots on a Bitpart server listening at `<BIND>` and with authorization token `<AUTH>`:

```
  bitpart-cli --auth <AUTH> --connect <BIND> list
```

## CSML

Bitpart's conversation logic is defined by scripts written in the open-source Conversational Markup Language, or CSML. Visit [the documentation from the CSML project](https://docs.csml.dev/) to learn how to write a CSML conversation flow. Each instance of Bitpart can run one or more bots, where each bot processes incoming messages according to one or more CSML flows.

## License

[<img src="https://www.gnu.org/graphics/agplv3-with-text-162x68.png" alt="AGPLv3" >](https://www.gnu.org/licenses/agpl-3.0.html)

Bitpart is a free software project licensed under the GNU Affero General Public License v3.0 (AGPLv3). Parts of it are derived from code from the [Presage](https://github.com/whisperfish/presage) and [CSML](https://csml.dev) projects, marked where appropriate.
