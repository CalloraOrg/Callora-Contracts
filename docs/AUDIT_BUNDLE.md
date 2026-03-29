# Auditor Onboarding Bundle

This document provides a comprehensive guide for security auditors reviewing the Callora Contracts codebase. It covers build instructions, testing procedures, coverage verification, WASM output validation, and security documentation references.

## Quick Start

```bash
# Clone and navigate to the project
git clone <repository-url>
cd Callora-Contracts

# Build all contracts
cargo build

# Run all tests
cargo test

# Generate coverage report (requires cargo-tarpaulin)
./scripts/coverage.sh

# Build WASM binaries for deployment
cargo build --target wasm32-unknown-unknown --release -p callora-vault
cargo build --target wasm32-unknown-unknown --release -p callora-settlement
cargo build --target wasm32-unknown-unknown --release -p callora-revenue-pool

# Verify WASM size constraints
./scripts/check-wasm-size.sh
```

## Project Overview

Callora Contracts is a Soroban (Stellar) smart contract suite for an API marketplace with prepaid vault functionality and revenue settlement. The system consists of three main contracts:

- **`callora-vault`**: Prepaid balance management with deposit/deduct operations
- **`callora-settlement`**: Revenue distribution and payment processing  
- **`callora-revenue-pool`**: Revenue pool management and distribution

## Prerequisites

### Required Tools

1. **Rust** (stable toolchain)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Soroban CLI** (for deployment testing)
   ```bash
   cargo install soroban-cli
   ```

3. **WASM target** (for release builds)
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

4. **cargo-tarpaulin** (for coverage reports)
   ```bash
   cargo install cargo-tarpaulin
   ```

### System Requirements

- Operating System: Linux, macOS, or Windows
- Rust version: 1.70+ (stable)
- Memory: 4GB+ recommended for WASM builds

## Build Instructions

### Development Build

```bash
# Build all contracts in development mode
cargo build

# Build specific contract
cargo build -p callora-vault
cargo build -p callora-settlement  
cargo build -p callora-revenue-pool
```

### Release Build (WASM)

```bash
# Build all contracts for deployment
cargo build --target wasm32-unknown-unknown --release

# Build specific contract
cargo build --target wasm32-unknown-unknown --release -p callora-vault
cargo build --target wasm32-unknown-unknown --release -p callora-settlement
cargo build --target wasm32-unknown-unknown --release -p callora-revenue-pool
```

### WASM Output Locations

After release build, WASM binaries are located at:
- `target/wasm32-unknown-unknown/release/callora_vault.wasm`
- `target/wasm32-unknown-unknown/release/callora_settlement.wasm`
- `target/wasm32-unknown-unknown/release/callora_revenue_pool.wasm`

### Size Verification

Soroban enforces a 64KB limit on contract size. Verify compliance:

```bash
# Automated size check for vault contract
./scripts/check-wasm-size.sh

# Manual verification
ls -la target/wasm32-unknown-unknown/release/*.wasm
```

Expected sizes (as of current version):
- Vault contract: ~17.5KB (well under 64KB limit)
- Settlement contract: ~15-20KB
- Revenue pool contract: ~15-20KB

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific contract
cargo test -p callora-vault
cargo test -p callora-settlement
cargo test -p callora-revenue-pool

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_deposit_success
```

### Test Coverage

The project enforces **95% minimum line coverage** on all code.

#### Generate Coverage Report

```bash
# Automated coverage with enforcement
./scripts/coverage.sh

# Manual coverage generation
cargo tarpaulin
```

#### Coverage Output

Reports are generated in the `coverage/` directory:
- `coverage/tarpaulin-report.html` - Interactive HTML report
- `coverage/cobertura.xml` - XML format for CI systems

#### Coverage Requirements

- **Minimum threshold**: 95% line coverage
- **Enforcement**: CI fails if coverage drops below threshold
- **Scope**: All contract code in `contracts/*/src/`
- **Exclusions**: Build scripts, generated code

### Test Categories

#### Unit Tests
- Located in `contracts/*/src/test.rs`
- Cover all public functions and edge cases
- Include panic scenarios with `#[should_panic]`
- Test arithmetic overflow/underflow conditions

#### Integration Tests
- Cross-contract interaction testing
- End-to-end user flow validation
- Event emission verification

#### Fuzz Tests
- Deterministic property-based testing
- Balance invariant validation
- Input validation stress testing

### Key Test Scenarios

#### Vault Contract Tests
- ✅ Initialization with various parameters
- ✅ Deposit authorization (owner/allowed depositor)
- ✅ Deduction with balance validation
- ✅ Batch operations with partial failures
- ✅ Withdrawal permissions and limits
- ✅ Arithmetic overflow/underflow protection
- ✅ Input validation (zero/negative amounts)
- ✅ Event emission verification

#### Settlement Contract Tests
- ✅ Payment processing and routing
- ✅ Developer balance management
- ✅ Pool distribution logic
- ✅ Authorization controls

#### Revenue Pool Tests
- ✅ Revenue distribution mechanisms
- ✅ Admin controls and permissions
- ✅ Balance tracking and validation

## Security Documentation

### Core Security Documents

1. **[SECURITY.md](../SECURITY.md)** - Comprehensive security checklist and audit recommendations
2. **[INVARIANTS.md](../INVARIANTS.md)** - Mathematical invariants and balance guarantees
3. **[EVENT_SCHEMA.md](../EVENT_SCHEMA.md)** - Event definitions for monitoring and indexing
4. **[ACCESS_CONTROL.md](ACCESS_CONTROL.md)** - Role-based access control documentation

### Security Highlights

#### Arithmetic Safety
- All balance operations use `checked_add`/`checked_sub`
- Overflow checks enabled in both dev and release profiles
- Input validation prevents zero/negative amounts
- Maximum balance: `i128::MAX` (≈ 1.7 × 10³⁸ stroops)

#### Access Control
- Owner-based permissions for privileged operations
- Optional allowed depositor for backend automation
- Authorization required for all state-changing operations
- Immediate access revocation capabilities

#### Input Validation
- Amount validation: `amount > 0` for all operations
- Balance sufficiency checks before deductions
- Parameter validation on initialization
- Boundary condition handling

#### Event Logging
- Comprehensive event emission for all state changes
- Structured event schema for indexing
- Full context logging for audit trails

## Expected PR Artifacts for Security-Sensitive Changes

When reviewing pull requests that affect security-sensitive areas, auditors should expect the following artifacts:

### Code Changes
- [ ] **Diff Review**: Clear, focused changes with minimal scope
- [ ] **Test Updates**: New tests covering changed functionality
- [ ] **Documentation Updates**: Updated security docs if applicable

### Testing Evidence
- [ ] **Test Results**: All tests passing with output logs
- [ ] **Coverage Report**: Maintained or improved coverage (≥95%)
- [ ] **Edge Case Testing**: Specific tests for security boundaries

### Security Analysis
- [ ] **Threat Model Updates**: Analysis of new attack vectors
- [ ] **Invariant Verification**: Proof that mathematical invariants hold
- [ ] **Access Control Review**: Verification of permission changes

### Build Artifacts
- [ ] **WASM Size Check**: Verification that contracts remain under 64KB
- [ ] **Release Build Success**: Clean compilation without warnings
- [ ] **Clippy Clean**: No linting warnings with strict settings

### Documentation
- [ ] **Security Checklist Updates**: Updated SECURITY.md if needed
- [ ] **Event Schema Changes**: Updated EVENT_SCHEMA.md for new events
- [ ] **API Documentation**: Updated function documentation

## Development Workflow

### Pre-commit Checks

Before submitting changes, run:

```bash
# Format code
cargo fmt

# Lint with strict settings
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
cargo test

# Verify coverage
./scripts/coverage.sh

# Check WASM builds
cargo build --target wasm32-unknown-unknown --release
./scripts/check-wasm-size.sh
```

### Branch Strategy

- Use descriptive branch names: `security/fix-overflow-check`, `audit/add-invariant-tests`
- Keep PRs focused and small for easier review
- Include security impact assessment in PR descriptions

## Deployment Verification

### WASM Binary Validation

```bash
# Verify WASM structure
wasm-objdump -h target/wasm32-unknown-unknown/release/callora_vault.wasm

# Check exports
wasm-objdump -j Export target/wasm32-unknown-unknown/release/callora_vault.wasm

# Validate size constraints
./scripts/check-wasm-size.sh
```

### Soroban Deployment Testing

```bash
# Deploy to testnet (requires Soroban CLI setup)
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/callora_vault.wasm \
  --source-account <account> \
  --network testnet

# Initialize contract
soroban contract invoke \
  --id <contract-id> \
  --source-account <account> \
  --network testnet \
  -- init \
  --owner <owner-address> \
  --usdc_token <usdc-token-address> \
  --initial_balance 0
```

## Troubleshooting

### Common Build Issues

#### WASM Target Missing
```bash
# Error: target 'wasm32-unknown-unknown' not found
rustup target add wasm32-unknown-unknown
```

#### Tarpaulin Installation Issues
```bash
# On Ubuntu/Debian
sudo apt-get install libssl-dev pkg-config

# On macOS
brew install openssl pkg-config
```

#### Size Limit Exceeded
If WASM binary exceeds 64KB:
1. Review `Cargo.toml` optimization settings
2. Check for unnecessary dependencies
3. Consider code refactoring to reduce binary size

### Test Failures

#### Coverage Below Threshold
```bash
# Generate detailed coverage report
cargo tarpaulin --out Html
# Open coverage/tarpaulin-report.html to identify uncovered lines
```

#### Arithmetic Overflow in Tests
- Verify `overflow-checks = true` in Cargo.toml
- Check test inputs for boundary conditions
- Ensure proper use of `checked_*` arithmetic operations

## Contact and Support

For questions about the audit process or technical issues:

1. **Repository Issues**: Create GitHub issues for bugs or questions
2. **Security Concerns**: Follow responsible disclosure in SECURITY.md
3. **Documentation**: Refer to linked security documents for detailed specifications

## Audit Checklist

Use this checklist to ensure comprehensive audit coverage:

### Pre-Audit Setup
- [ ] Environment setup complete (Rust, Soroban CLI, cargo-tarpaulin)
- [ ] Repository cloned and builds successfully
- [ ] All tests pass locally
- [ ] Coverage report generated (≥95%)
- [ ] WASM binaries build and pass size checks

### Code Review
- [ ] All contracts reviewed for security vulnerabilities
- [ ] Access control mechanisms validated
- [ ] Arithmetic operations checked for overflow/underflow
- [ ] Input validation comprehensive
- [ ] Event emission complete and accurate

### Testing Review
- [ ] Test coverage meets requirements
- [ ] Edge cases and boundary conditions tested
- [ ] Panic scenarios properly tested
- [ ] Integration tests cover user flows
- [ ] Fuzz tests validate invariants

### Documentation Review
- [ ] Security documentation complete and accurate
- [ ] Event schema matches implementation
- [ ] Access control model clearly documented
- [ ] Invariants mathematically sound

### Deployment Readiness
- [ ] WASM binaries optimized and under size limits
- [ ] Deployment procedures documented
- [ ] Monitoring and alerting considerations addressed
- [ ] Upgrade and migration paths defined

This audit bundle provides the foundation for a thorough security review of the Callora Contracts codebase. Auditors should use this as a starting point and expand their analysis based on specific security requirements and threat models.