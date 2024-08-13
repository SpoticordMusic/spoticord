# Spoticord

Spoticord is a Discord music bot that allows you to control your music using the Spotify app.
Spoticord is built on top of [librespot](https://github.com/librespot-org/librespot) (with tiny additional changes), to allow full control using the Spotify client, with [serenity](https://github.com/serenity-rs/serenity) and [songbird](https://github.com/serenity-rs/songbird) for Discord communication.
Being built on top of rust, Spoticord is relatively lightweight and can run on low-spec hardware.

## How to use

### Official bot

Spoticord is being hosted as an official bot. You can find more info about how to use this bot over at [the Spoticord website](https://spoticord.com/).

### Environment variables

Spoticord uses environment variables to configure itself. The following variables are required:

- `DISCORD_TOKEN`: The Discord bot token used for authenticating with Discord.
- `DATABASE_URL`: The URL of the postgres database where spoticord will store user data. Currently only postgresql databases are supported.
- `LINK_URL`: The base URL of the account-linking frontend used for authenticating users with Spotify. This base URL must point to an instance of [the Spoticord Link frontend](https://github.com/SpoticordMusic/spoticord-link).
- `SPOTIFY_CLIENT_ID`: The Spotify Client ID for the Spotify application that is used for Spoticord. This will be used for refreshing tokens.
- `SPOTIFY_CLIENT_SECRET`: The Spotify Client Secret for the Spotify application that is used for Spoticord. This will be used for refreshing tokens.

Additionally you can configure the following variables:

- `GUILD_ID`: The ID of the Discord server where this bot will create commands for. This is used during testing to prevent the bot from creating slash commands in other servers, as well as generally being faster than global command propagation. This variable is required when running a debug build, and ignored when running a release build.
- `KV_URL`: The connection URL of a redis-server instance used for storing realtime data. This variable is required when compiling with the `stats` feature.

#### Providing environment variables

You can provide environment variables in a `.env` file at the root of the working directory of Spoticord.
You can also provide environment variables the normal way, e.g. the command line, using `export` (or `set` for Windows) or using docker.
Environment variables set this way take precedence over those in the `.env` file (if one exists).

# Compiling

For information about how to compile Spoticord from source, check out [COMPILING.md](COMPILING.md).

# Contributing

For information about how to contribute to Spoticord, check out [CONTRIBUTING.md](CONTRIBUTING.md).

# Contact

![Discord Shield](https://discordapp.com/api/guilds/779292533053456404/widget.png?style=shield)

If you have any questions, feel free to join the [Spoticord Discord server](https://discord.gg/wRCyhVqBZ5)!

# License

Spoticord is licensed under the [GNU Affero General Public License v3.0](LICENSE).
