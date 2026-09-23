# Bitpart 🤖

[![Run checks](https://github.com/throneless-tech/bitpart/actions/workflows/run_checks.yaml/badge.svg)](https://github.com/throneless-tech/bitpart/actions/workflows/run_checks.yaml)

Bitpart is a messaging tool that runs on top of Signal to support activists, journalists, and human rights defenders.

## Building

This repository contains three components: the Bitpart server, a command-line client for connecting to the Bitpart server, and a storage adapter used for storing Signal key information to Bitpart's database. In order to build all three components, make sure you have a _2024 edition of Rust_ and run:

```
  cargo build --release
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

### Adding a bot and connecting it to Signal

When you have the server running as described above, and you have `bitpart-cli` able to connect to it at location `<BIND>` with authorization token `<AUTH>`, you can add a bot:

```
  bitpart-cli --auth <AUTH> --connect <BIND> add --id <BOT_ID> ./<BASENAME>.csml
```

where `<BOT_ID>` is a unique name you choose for the bot, and `<BASENAME>.csml` is the bot's CSML script. You can include multiple CSML scripts; if you do, pass `--default <BASENAME>` to choose which one is used by default for new conversations. To find out more about CSML scripting, check out the example(s) in the `examples` directory in this repository.

To have new conversations with the bot use Signal disappearing messages, add `--expire-timer <SECONDS>` (for example `604800` for one week). The timer is set when a user first contacts the bot; after that, any change the user makes to the timer in their Signal app is left in place. When you add a new version of an existing bot without `--expire-timer`, it keeps the previous version's timer; use `--expire-timer 0` to remove it.

If the bot's Signal account is added to a group, it only responds to messages that @mention it. Each group member has their own conversation with the bot in that group, separate from any direct conversation they have with it, and the bot's replies are sent to the group.

To link the bot you created to Signal so that it receives messages, you must open a _channel_ between the bot and a Signal account. **We recommend using a separate Signal account just for this purpose**, since Bitpart will also receive and respond to the Signal messages sent to this account.

```
  bitpart-cli --auth <AUTH> --connect <BIND> channel-link --id <CHANNEL_ID> --bot-id <BOT_ID> --device-name <DEVICE_NAME> [--profile-name <PROFILE_NAME>]
```

where `<CHANNEL_ID>` is a name you choose for this Signal account within the bot (for example, `signal`), `<BOT_ID>` is the id of the bot you're linking, and `<DEVICE_NAME>` is the name of the device as it will appear in the list of linked devices on Signal (for example, `bitpart`). If you pass `--profile-name`, the Signal profile name of the linked account is set to `<PROFILE_NAME>` once linking finishes; because the bot is linked as a device of that account, this changes the name shown for the account everywhere, including on the device you linked it from.

A bot can have several channels, each linked to a different Signal account; give each one its own `<CHANNEL_ID>`. Conversations are kept separately per channel, so someone who messages two of a bot's accounts has a separate conversation with each. To relink a channel whose Signal account was disconnected, run the same command again with the same `<CHANNEL_ID>`: only that channel is stopped and relinked, and the bot's other channels keep running. Relinking discards that channel's Signal keys and sessions, since Signal issues new ones for the new link, but its conversations with users are kept.

After you enter this command, a QR code will be displayed. In your Signal client, go to _Settings -> Linked Devices -> Link new device_ and take a picture of the QR code. After a few seconds, your bot will finish linking with Signal. From another Signal device, send a message to the number or username associated with the bot (**NOTE**: unless you pass `--profile-name`, the bot will take on the profile information of the linked device) and it should reply!

## CSML

Bitpart's conversation logic is defined by scripts written in the open-source Conversational Standard Meta Language, or CSML. Visit [the documentation from the CSML project](https://docs.csml.dev/) to learn how to write a CSML conversation flow. Each instance of Bitpart can run one or more bots, where each bot processes incoming messages according to one or more CSML flows.

## License

[<img src="https://www.gnu.org/graphics/agplv3-with-text-162x68.png" alt="AGPLv3" >](https://www.gnu.org/licenses/agpl-3.0.html)

Bitpart is a free software project licensed under the GNU Affero General Public License v3.0 (AGPLv3). Parts of it are derived from code from the [Presage](https://github.com/whisperfish/presage) and [CSML](https://csml.dev) projects, marked where appropriate.
