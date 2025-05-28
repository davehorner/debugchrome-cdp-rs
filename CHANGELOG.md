# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.10](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.9...v0.1.10) - 2025-05-28

### Added

- add .gitignore and remove unnecessary dependencies from Cargo.toml and Cargo.lock

## [0.1.9](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.8...v0.1.9) - 2025-05-07

### Fixed

-a broken build

## [0.1.8](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.7...v0.1.8) - 2025-05-07

### Added

- *(gui)* add egui-snarl graph viewer and jokes module

## [0.1.7](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.6...v0.1.7) - 2025-05-05

### Added

- bangId is now primarily found and set in the hash #.  initial load

## [0.1.6](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.5...v0.1.6) - 2025-05-04

### Added

- add futures dependency and refactor async functions in main.rs to improve asynchronous handling and timeouts in WebSocket communication

## [0.1.5](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.4...v0.1.5) - 2025-05-04

### Added

- start DPI scaling support (not working now), specifying location

## [0.1.4](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.3...v0.1.4) - 2025-05-04

### Other

- update README with closing behavior and new window opening examples

## [0.1.3](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.2...v0.1.3) - 2025-05-04

### Fixed

- update examples in README and improve screenshot handling in main.rs

## [0.1.2](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.1...v0.1.2) - 2025-05-04

### Fixed

- update README with enhancements and clarify features, modify main.rs for improved logging and remove deprecated code

## [0.1.1](https://github.com/davehorner/debugchrome-cdp-rs/compare/v0.1.0...v0.1.1) - 2025-05-04

### Fixed

- update README.md with new debugchrome URL format and correct pluralization, change temporary directory name in main.rs to debugchrome

## [0.1.0](https://github.com/davehorner/debugchrome-cdp-rs/releases/tag/v0.1.0) - 2025-05-03

### Added

- update dependencies and enhance functionality for handling Chrome tabs with bangId, including adding new features for starting, closing, refreshing tabs, and improved window management
- async, all parameters are bang!
- add initial implementation for debugchrome-cdp-rs with Cargo files and README

### Other

- Initial commit
