# Changelog

## 2.1.2 | September 28th 2023

### Changes
* Removed OpenSSL dependency
* Added aarch64 support
* Added cross compilation to Github Actions
* Added `dev` branch to Github Actions
* Removed hardcoded URL in the /join command
* Fixed an issue in /playing where the bot showed it was playing even though it was paused

**Full Changelog**: https://github.com/SpoticordMusic/spoticord/compare/v2.1.1...v2.1.2

## 2.1.1 | September 23rd 2023
Reduced the amount of CPU that the bot uses from ~15%-25% per user to 1%-2% per user (percentage per core, benched on an AMD Ryzen 9 5950X).

### Changes
* Fixed issue #20

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