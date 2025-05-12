# PucksData

NHL Stats Engine CLI - A tool for fetching and caching NHL data.

## Overview

PucksData is a command-line tool for fetching, caching, and processing NHL data. It supports various data types including games, players, teams, and seasons. All data is cached locally in the data/raw directory for offline access.

## Usage

```bash
# Get game story for a specific game
pucksdata game story 2023020001

# Get player summary
pucksdata player summary 8478402

# Get team statistics
pucksdata team current-stats EDM

# Get current standings
pucksdata team standings-now

# Get full help
pucksdata --help
```

## Testing

PucksData includes a comprehensive test suite to ensure API connectivity and functionality. To run the tests:

```bash
# Run all tests
cargo test

# Run only the API tests
cargo test --test api_tests

# Run tests with output
cargo test -- --nocapture

# Run all endpoint tests (hits real APIs)
cargo test --test endpoint_tests
```

### Test Categories

1. **API Tests** - Tests basic API functionality and error handling
2. **Cache Tests** - Tests file caching mechanisms
3. **Endpoint Tests** - Tests connectivity to all supported NHL API endpoints
4. **CLI Tests** - Tests command-line interface functionality
5. **Mock API Tests** - Placeholder for future mock-based testing

### Testing Notes

- Endpoint tests will hit the real NHL API and are designed to track which endpoints return 404s vs. other errors
- Some endpoints may fail with 404 Not Found if they're season-specific or not currently active
- The test summary provides a breakdown of successful vs. failed endpoints

## License

MIT
