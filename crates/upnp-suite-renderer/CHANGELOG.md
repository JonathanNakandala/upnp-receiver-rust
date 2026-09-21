# Changelog

## [0.3.0](https://github.com/JonathanNakandala/upnp-receiver-rust/compare/upnp-suite-renderer-v0.2.0...upnp-suite-renderer-v0.3.0) (2026-09-21)


### Features

* **client:** implement gena subscription for lastchange ([23e9a29](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/23e9a29a9e7a21d50cf5a1d519320f896fd85fdb))
* **gstreamer:** implement video support ([#9](https://github.com/JonathanNakandala/upnp-receiver-rust/issues/9)) ([dcb0cc7](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/dcb0cc796d0b5b7a6e354ecb011007a72b50d065))
* initial implementation of client ([437f175](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/437f1751dfd98a8eff281dc5d891ef50c18150d0))
* **renderer:** ConnectionManager:3 upgrade ([#8](https://github.com/JonathanNakandala/upnp-receiver-rust/issues/8)) ([5477f8b](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/5477f8bad5ad77e6c3da8448cf8d89524f8b3706))
* **renderer:** playback controls for video ([96c5b47](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/96c5b47142fe3de40f6d85c8a3f309c0efab78a6))


### Bug Fixes

* **gstreamer:** put on separate thread ([466974a](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/466974a9bbd777fbe8682b8ecf2c2afe5573a03a))
* **upnp-suite-renderer:** use tokio::task to poll Gstreamer ([ea59ca6](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/ea59ca601e372650bb4a950b9fd906ae6deeaf1d))

## [0.2.0](https://github.com/JonathanNakandala/upnp-receiver-rust/compare/upnp-suite-renderer-v0.1.0...upnp-suite-renderer-v0.2.0) (2026-09-20)


### Features

* initial implementation ([6ab297c](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/6ab297ce3afa9ab401cf0af42b617f4cd2b05acf))
