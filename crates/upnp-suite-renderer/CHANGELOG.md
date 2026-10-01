# Changelog

## [0.5.0](https://github.com/JonathanNakandala/upnp-receiver-rust/compare/upnp-suite-renderer-v0.4.0...upnp-suite-renderer-v0.5.0) (2026-10-01)


### Features

* brightness / contrast control ([38e9c45](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/38e9c4539c0cb560e72d384bef532b3c230e28ac))
* **gstreamer:** implement Horizontal/Vertical Keystone ([c201ff3](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/c201ff37b070f3ce08cdab3df752f56d26b213a9))
* **gstreamer:** implement Loudness compensation ([230647f](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/230647f8d3f0d23e5058cca4d41cebe077abd944))
* **gstreamer:** implement sharpness control ([48616ec](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/48616ec9fcfdaab05815d6855deccc5c3ceeb3f0))
* **gstreamer:** RGB Gain/BlackLevel and ColorTemperature ([e9343fd](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/e9343fd0d3fd46afd7cfa6e12c833c5e90a1d2eb))
* **rendering-control:** implement GetStateVariables ([270bff0](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/270bff0149284431cfce85430e6fa7c9988a2e63))
* **rendering-control:** implement SetStateVariables ([6bef265](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/6bef265d9072dcda32ac1a894d574ed051e53a4c))
* **RenderingControl:** advertise v3 ([38b643e](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/38b643e46f34675a350c015adcb10c624c45affd))
* **RenderingControl:** GetAllowedTransforms / GetTransforms / SetTransforms / GetAllAvailableTransforms ([8731a6a](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/8731a6a2d1b957a261e9defbc73e582e89c595c9))


### Bug Fixes

* **gstreamer:** resize and scale fullscreen video correctly ([63f0988](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/63f098838ed1c52da7d511be834b645a6f82e58c))

## [0.4.0](https://github.com/JonathanNakandala/upnp-receiver-rust/compare/upnp-suite-renderer-v0.3.1...upnp-suite-renderer-v0.4.0) (2026-09-21)


### Features

* **gsstreamer:** auto close and open windows when playing video ([cf84a88](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/cf84a8886cd02ab56b0dac746471447b1d29bb32))
* **gstreamer:** adaptive prefetching and seek cancel to cache ([0193ce1](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/0193ce1a9731992577aa789747b9eb0eec86785d))
* **gstreamer:** handle 10bit video and YUV-&gt;RGB on GPU ([2ad4e46](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/2ad4e46523627ce20ab884c77234a507a9ad4961))
* **gstreamer:** in-memory lru cache replacing entire video download ([67f0d8f](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/67f0d8f77e933179e41ec816a45edd15b1b9e0f8))
* **gstreamer:** removed glupload, glcolorconvert, gldownload stages ([0a39e29](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/0a39e29c400808a6615e24d2c1d70295c9c688e2))


### Bug Fixes

* **gstreamer:** use run_on_gst_thread mechanism for recovery ([40fa918](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/40fa91896fadb0be9ae6b95aed74fee98addf404))
* **gstreamer:** use separate compositor for subtitles ([fa6accb](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/fa6accb219439bff388e19abd9ec6abb26bf1f10))
* **gstreamer:** video/audio pipeline switching and add decoder-stall recovery ([#19](https://github.com/JonathanNakandala/upnp-receiver-rust/issues/19)) ([8f5640c](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/8f5640cbd2f599676a1bd9483db6f22e9f2cc048))

## [0.3.1](https://github.com/JonathanNakandala/upnp-receiver-rust/compare/upnp-suite-renderer-v0.3.0...upnp-suite-renderer-v0.3.1) (2026-09-21)


### Bug Fixes

* **gstreamer:** maintain aspect ratio in fullscreen ([#14](https://github.com/JonathanNakandala/upnp-receiver-rust/issues/14)) ([943a6b9](https://github.com/JonathanNakandala/upnp-receiver-rust/commit/943a6b9f8dbc409e02dcaf687c7a74941b7ce65d))

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
