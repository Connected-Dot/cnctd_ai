# Changelog

All notable changes to this project will be documented in this file.

## [0.1.5] - 2025-11-20

### Fixed
- Fixed tool calling examples to work with `rmcp::model::Tool` type
- Fixed error handling example to properly construct Tool schemas
- Fixed streaming tool calling example to use correct Tool structure
- All examples now compile and run correctly

### Added
- Added `tool_helpers` module with `create_tool()` and `create_tool_borrowed()` helper functions
- Added comprehensive README with usage examples
- Added CHANGELOG to track changes

### Changed
- Simplified tool creation with helper functions instead of manual Arc/Cow construction
- Examples now use cleaner, more ergonomic API for tool creation
- Updated all tool-related examples to use the new helper functions

## [0.1.4] - Previous Version

### Changed
- Migrated from custom Tool struct to `rmcp::model::Tool`
- This breaking change required updates to all tool-related code
