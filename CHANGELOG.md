# Changelog

## 2.2.6 | November 13th 2024

- Updated voice module to support Discord's new mandatory voice encryption

## 2.2.5 | October 18th 2024

- Updated librespot to rel 0.5.0 (was: 0.5.0-dev)
- Fixed an issue where Spoticord would lose connection to Spotify servers (fixed by librespot upgrade)
- Reworked authentication logic, hopefully reducing the amount of "suspicious login" forced password resets

## 2.2.4 | September 30th 2024

- Added a message for if the Spotify AP connection drops
- Added additional timeouts to credential retrieval
- Removed multiple points of failure in `librespot` that could shut down the bot
- Fixed an issue where non-premium users could crash the bot for everyone (See point 3)

## 2.2.3 | September 20th 2024

- Made backend changes to librespot to prevent deadlocking by not waiting for thread shutdowns
- Added retrying to Spotify login logic to reduce the chance of the bot failing to connect

## 2.2.2 | September 2nd 2024

- Added backtrace logging to player creation to figure out some mystery crashes

## 2.2.1 | August 22nd 2024

- Added new option: `/playing` can now receive an updating behavior parameter
- Added album name to `/playing` embed
- Fixed a bug where uncached guilds would panic the bot
- Fixed small issue with embed styling
- Updated to Rust 1.80.1 (from 1.79.0)
- Updated `diesel` and addons to latest versions
- Removed `lazy_static` in favor of `LazyLock` (Rust 1.80.0+ feature)
- Bumped MSRV to 1.80.0 due to the introduction of `LazyLock`

## 2.2.0 | August 13th 2024

### Changes

- Rewrote the entire bot (again)
- Updated librespot from v0.4.2 to v0.5.0-dev
- Added `/lyrics`, which provides the user with an auto-updating lyrics embed
- Added `/stop`, which disconnects the bot from Spotify without leaving the call (will still leave after 5 minutes)
- Changed `/playing` to automatically update the embed accordingly
- Renamed `/leave` to `/disconnect`
- Removed the Database API, replaced with direct connection to a Postgres database

**Full Changelog** (good luck): https://github.com/SpoticordMusic/spoticord/compare/v2.1.2..v2.2.0

## 2.1.2 | September 28th 2023

### Changes

- Removed OpenSSL dependency
- Added aarch64 support
- Added cross compilation to Github Actions
- Added `dev` branch to Github Actions
- Removed hardcoded URL in the /join command
- Fixed an issue in /playing where the bot showed it was playing even though it was paused

**Full Changelog**: https://github.com/SpoticordMusic/spoticord/compare/v2.1.1...v2.1.2

## 2.1.1 | September 23rd 2023

Reduced the amount of CPU that the bot uses from ~15%-25% per user to 1%-2% per user (percentage per core, benched on an AMD Ryzen 9 5950X).

### Changes

- Fixed issue #20

**Full Changelog**: https://github.com/SpoticordMusic/spoticord/compare/v2.1.0...v2.1.1

## 2.1.0 | September 20th 2023

So, it's been a while since I worked on this project, and some bugs have since been discovered.
The main focus for this version is to stop using multiple processes for every player, and instead do everything in threads.

### Changes

- Remove metrics, as I wasn't using this feature anyways
- Bring back KV for storing total/active sessions, as prometheus is no longer being used
- Allocate new players in-memory, instead of using subprocesses
- Fix issue #17
- Fix some issues with the auto-disconnect
- Removed the automatic device switching on bot join, which was causing some people to not be able to use the bot
- Force communication through the closest Spotify AP, reducing latency
- Potential jitter reduction
- Enable autoplay
- After skipping a song, you will no longer hear a tiny bit of the previous song after the silence

**Full Changelog**: https://github.com/SpoticordMusic/spoticord/compare/v2.0.0...v2.1.0

### Issues

- Currently, the CPU usage is much higher than it used to be. I really wanted to push this update out before taking the time to do some optimizations, as the bot and server are still easily able to hold up the limited amount of Spoticord users (and v2.0.0 was just falling apart). Issue is being tracked in #20

## 2.0.0 | June 8th 2023

- Initial Release
