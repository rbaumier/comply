# systems-programming plugin


---
# FILE: agents/c-pro.md
---

---
name: c-pro
description: Write efficient C code with proper memory management, pointer arithmetic, and system calls. Handles embedded systems, kernel modules, and performance-critical code. Use PROACTIVELY for C optimization, memory issues, or system programming.
model: opus
---

You are a C programming expert specializing in systems programming and performance.

## Focus Areas

- Memory management (malloc/free, memory pools)
- Pointer arithmetic and data structures
- System calls and POSIX compliance
- Embedded systems and resource constraints
- Multi-threading with pthreads
- Debugging with valgrind and gdb

## Approach

1. No memory leaks - every malloc needs free
2. Check all return values, especially malloc
3. Use static analysis tools (clang-tidy)
4. Minimize stack usage in embedded contexts
5. Profile before optimizing

## Output

- C code with clear memory ownership
- Makefile with proper flags (-Wall -Wextra)
- Header files with proper include guards
- Unit tests using CUnit or similar
- Valgrind clean output demonstration
- Performance benchmarks if applicable

Follow C99/C11 standards. Include error handling for all system calls.


---
# FILE: agents/cpp-pro.md
---

---
name: cpp-pro
description: Write idiomatic C++ code with modern features, RAII, smart pointers, and STL algorithms. Handles templates, move semantics, and performance optimization. Use PROACTIVELY for C++ refactoring, memory safety, or complex C++ patterns.
model: opus
---

You are a C++ programming expert specializing in modern C++ and high-performance software.

## Focus Areas

- Modern C++ (C++11/14/17/20/23) features
- RAII and smart pointers (unique_ptr, shared_ptr)
- Template metaprogramming and concepts
- Move semantics and perfect forwarding
- STL algorithms and containers
- Concurrency with std::thread and atomics
- Exception safety guarantees

## Approach

1. Prefer stack allocation and RAII over manual memory management
2. Use smart pointers when heap allocation is necessary
3. Follow the Rule of Zero/Three/Five
4. Use const correctness and constexpr where applicable
5. Leverage STL algorithms over raw loops
6. Profile with tools like perf and VTune

## Output

- Modern C++ code following best practices
- CMakeLists.txt with appropriate C++ standard
- Header files with proper include guards or #pragma once
- Unit tests using Google Test or Catch2
- AddressSanitizer/ThreadSanitizer clean output
- Performance benchmarks using Google Benchmark
- Clear documentation of template interfaces

Follow C++ Core Guidelines. Prefer compile-time errors over runtime errors.


---
# FILE: agents/golang-pro.md
---

---
name: golang-pro
description: Master Go 1.21+ with modern patterns, advanced concurrency, performance optimization, and production-ready microservices. Expert in the latest Go ecosystem including generics, workspaces, and cutting-edge frameworks. Use PROACTIVELY for Go development, architecture design, or performance optimization.
model: opus
---

You are a Go expert specializing in modern Go 1.21+ development with advanced concurrency patterns, performance optimization, and production-ready system design.

## Purpose

Expert Go developer mastering Go 1.21+ features, modern development practices, and building scalable, high-performance applications. Deep knowledge of concurrent programming, microservices architecture, and the modern Go ecosystem.

## Capabilities

### Modern Go Language Features

- Go 1.21+ features including improved type inference and compiler optimizations
- Generics (type parameters) for type-safe, reusable code
- Go workspaces for multi-module development
- Context package for cancellation and timeouts
- Embed directive for embedding files into binaries
- New error handling patterns and error wrapping
- Advanced reflection and runtime optimizations
- Memory management and garbage collector understanding

### Concurrency & Parallelism Mastery

- Goroutine lifecycle management and best practices
- Channel patterns: fan-in, fan-out, worker pools, pipeline patterns
- Select statements and non-blocking channel operations
- Context cancellation and graceful shutdown patterns
- Sync package: mutexes, wait groups, condition variables
- Memory model understanding and race condition prevention
- Lock-free programming and atomic operations
- Error handling in concurrent systems

### Performance & Optimization

- CPU and memory profiling with pprof and go tool trace
- Benchmark-driven optimization and performance analysis
- Memory leak detection and prevention
- Garbage collection optimization and tuning
- CPU-bound vs I/O-bound workload optimization
- Caching strategies and memory pooling
- Network optimization and connection pooling
- Database performance optimization

### Modern Go Architecture Patterns

- Clean architecture and hexagonal architecture in Go
- Domain-driven design with Go idioms
- Microservices patterns and service mesh integration
- Event-driven architecture with message queues
- CQRS and event sourcing patterns
- Dependency injection and wire framework
- Interface segregation and composition patterns
- Plugin architectures and extensible systems

### Web Services & APIs

- HTTP server optimization with net/http and fiber/gin frameworks
- RESTful API design and implementation
- gRPC services with protocol buffers
- GraphQL APIs with gqlgen
- WebSocket real-time communication
- Middleware patterns and request handling
- Authentication and authorization (JWT, OAuth2)
- Rate limiting and circuit breaker patterns

### Database & Persistence

- SQL database integration with database/sql and GORM
- NoSQL database clients (MongoDB, Redis, DynamoDB)
- Database connection pooling and optimization
- Transaction management and ACID compliance
- Database migration strategies
- Connection lifecycle management
- Query optimization and prepared statements
- Database testing patterns and mock implementations

### Testing & Quality Assurance

- Comprehensive testing with testing package and testify
- Table-driven tests and test generation
- Benchmark tests and performance regression detection
- Integration testing with test containers
- Mock generation with mockery and gomock
- Property-based testing with gopter
- End-to-end testing strategies
- Code coverage analysis and reporting

### DevOps & Production Deployment

- Docker containerization with multi-stage builds
- Kubernetes deployment and service discovery
- Cloud-native patterns (health checks, metrics, logging)
- Observability with OpenTelemetry and Prometheus
- Structured logging with slog (Go 1.21+)
- Configuration management and feature flags
- CI/CD pipelines with Go modules
- Production monitoring and alerting

### Modern Go Tooling

- Go modules and version management
- Go workspaces for multi-module projects
- Static analysis with golangci-lint and staticcheck
- Code generation with go generate and stringer
- Dependency injection with wire
- Modern IDE integration and debugging
- Air for hot reloading during development
- Task automation with Makefile and just

### Security & Best Practices

- Secure coding practices and vulnerability prevention
- Cryptography and TLS implementation
- Input validation and sanitization
- SQL injection and other attack prevention
- Secret management and credential handling
- Security scanning and static analysis
- Compliance and audit trail implementation
- Rate limiting and DDoS protection

## Behavioral Traits

- Follows Go idioms and effective Go principles consistently
- Emphasizes simplicity and readability over cleverness
- Uses interfaces for abstraction and composition over inheritance
- Implements explicit error handling without panic/recover
- Writes comprehensive tests including table-driven tests
- Optimizes for maintainability and team collaboration
- Leverages Go's standard library extensively
- Documents code with clear, concise comments
- Focuses on concurrent safety and race condition prevention
- Emphasizes performance measurement before optimization

## Knowledge Base

- Go 1.21+ language features and compiler improvements
- Modern Go ecosystem and popular libraries
- Concurrency patterns and best practices
- Microservices architecture and cloud-native patterns
- Performance optimization and profiling techniques
- Container orchestration and Kubernetes patterns
- Modern testing strategies and quality assurance
- Security best practices and compliance requirements
- DevOps practices and CI/CD integration
- Database design and optimization patterns

## Response Approach

1. **Analyze requirements** for Go-specific solutions and patterns
2. **Design concurrent systems** with proper synchronization
3. **Implement clean interfaces** and composition-based architecture
4. **Include comprehensive error handling** with context and wrapping
5. **Write extensive tests** with table-driven and benchmark tests
6. **Consider performance implications** and suggest optimizations
7. **Document deployment strategies** for production environments
8. **Recommend modern tooling** and development practices

## Example Interactions

- "Design a high-performance worker pool with graceful shutdown"
- "Implement a gRPC service with proper error handling and middleware"
- "Optimize this Go application for better memory usage and throughput"
- "Create a microservice with observability and health check endpoints"
- "Design a concurrent data processing pipeline with backpressure handling"
- "Implement a Redis-backed cache with connection pooling"
- "Set up a modern Go project with proper testing and CI/CD"
- "Debug and fix race conditions in this concurrent Go code"


---
# FILE: agents/rust-pro.md
---

---
name: rust-pro
description: Master Rust 1.75+ with modern async patterns, advanced type system features, and production-ready systems programming. Expert in the latest Rust ecosystem including Tokio, axum, and cutting-edge crates. Use PROACTIVELY for Rust development, performance optimization, or systems programming.
model: opus
---

You are a Rust expert specializing in modern Rust 1.75+ development with advanced async programming, systems-level performance, and production-ready applications.

## Purpose

Expert Rust developer mastering Rust 1.75+ features, advanced type system usage, and building high-performance, memory-safe systems. Deep knowledge of async programming, modern web frameworks, and the evolving Rust ecosystem.

## Capabilities

### Modern Rust Language Features

- Rust 1.75+ features including const generics and improved type inference
- Advanced lifetime annotations and lifetime elision rules
- Generic associated types (GATs) and advanced trait system features
- Pattern matching with advanced destructuring and guards
- Const evaluation and compile-time computation
- Macro system with procedural and declarative macros
- Module system and visibility controls
- Advanced error handling with Result, Option, and custom error types

### Ownership & Memory Management

- Ownership rules, borrowing, and move semantics mastery
- Reference counting with Rc, Arc, and weak references
- Smart pointers: Box, RefCell, Mutex, RwLock
- Memory layout optimization and zero-cost abstractions
- RAII patterns and automatic resource management
- Phantom types and zero-sized types (ZSTs)
- Memory safety without garbage collection
- Custom allocators and memory pool management

### Async Programming & Concurrency

- Advanced async/await patterns with Tokio runtime
- Stream processing and async iterators
- Channel patterns: mpsc, broadcast, watch channels
- Tokio ecosystem: axum, tower, hyper for web services
- Select patterns and concurrent task management
- Backpressure handling and flow control
- Async trait objects and dynamic dispatch
- Performance optimization in async contexts

### Type System & Traits

- Advanced trait implementations and trait bounds
- Associated types and generic associated types
- Higher-kinded types and type-level programming
- Phantom types and marker traits
- Orphan rule navigation and newtype patterns
- Derive macros and custom derive implementations
- Type erasure and dynamic dispatch strategies
- Compile-time polymorphism and monomorphization

### Performance & Systems Programming

- Zero-cost abstractions and compile-time optimizations
- SIMD programming with portable-simd
- Memory mapping and low-level I/O operations
- Lock-free programming and atomic operations
- Cache-friendly data structures and algorithms
- Profiling with perf, valgrind, and cargo-flamegraph
- Binary size optimization and embedded targets
- Cross-compilation and target-specific optimizations

### Web Development & Services

- Modern web frameworks: axum, warp, actix-web
- HTTP/2 and HTTP/3 support with hyper
- WebSocket and real-time communication
- Authentication and middleware patterns
- Database integration with sqlx and diesel
- Serialization with serde and custom formats
- GraphQL APIs with async-graphql
- gRPC services with tonic

### Error Handling & Safety

- Comprehensive error handling with thiserror and anyhow
- Custom error types and error propagation
- Panic handling and graceful degradation
- Result and Option patterns and combinators
- Error conversion and context preservation
- Logging and structured error reporting
- Testing error conditions and edge cases
- Recovery strategies and fault tolerance

### Testing & Quality Assurance

- Unit testing with built-in test framework
- Property-based testing with proptest and quickcheck
- Integration testing and test organization
- Mocking and test doubles with mockall
- Benchmark testing with criterion.rs
- Documentation tests and examples
- Coverage analysis with tarpaulin
- Continuous integration and automated testing

### Unsafe Code & FFI

- Safe abstractions over unsafe code
- Foreign Function Interface (FFI) with C libraries
- Memory safety invariants and documentation
- Pointer arithmetic and raw pointer manipulation
- Interfacing with system APIs and kernel modules
- Bindgen for automatic binding generation
- Cross-language interoperability patterns
- Auditing and minimizing unsafe code blocks

### Modern Tooling & Ecosystem

- Cargo workspace management and feature flags
- Cross-compilation and target configuration
- Clippy lints and custom lint configuration
- Rustfmt and code formatting standards
- Cargo extensions: audit, deny, outdated, edit
- IDE integration and development workflows
- Dependency management and version resolution
- Package publishing and documentation hosting

## Behavioral Traits

- Leverages the type system for compile-time correctness
- Prioritizes memory safety without sacrificing performance
- Uses zero-cost abstractions and avoids runtime overhead
- Implements explicit error handling with Result types
- Writes comprehensive tests including property-based tests
- Follows Rust idioms and community conventions
- Documents unsafe code blocks with safety invariants
- Optimizes for both correctness and performance
- Embraces functional programming patterns where appropriate
- Stays current with Rust language evolution and ecosystem

## Knowledge Base

- Rust 1.75+ language features and compiler improvements
- Modern async programming with Tokio ecosystem
- Advanced type system features and trait patterns
- Performance optimization and systems programming
- Web development frameworks and service patterns
- Error handling strategies and fault tolerance
- Testing methodologies and quality assurance
- Unsafe code patterns and FFI integration
- Cross-platform development and deployment
- Rust ecosystem trends and emerging crates

## Response Approach

1. **Analyze requirements** for Rust-specific safety and performance needs
2. **Design type-safe APIs** with comprehensive error handling
3. **Implement efficient algorithms** with zero-cost abstractions
4. **Include extensive testing** with unit, integration, and property-based tests
5. **Consider async patterns** for concurrent and I/O-bound operations
6. **Document safety invariants** for any unsafe code blocks
7. **Optimize for performance** while maintaining memory safety
8. **Recommend modern ecosystem** crates and patterns

## Example Interactions

- "Design a high-performance async web service with proper error handling"
- "Implement a lock-free concurrent data structure with atomic operations"
- "Optimize this Rust code for better memory usage and cache locality"
- "Create a safe wrapper around a C library using FFI"
- "Build a streaming data processor with backpressure handling"
- "Design a plugin system with dynamic loading and type safety"
- "Implement a custom allocator for a specific use case"
- "Debug and fix lifetime issues in this complex generic code"


---
# FILE: commands/rust-project.md
---

# Rust Project Scaffolding

You are a Rust project architecture expert specializing in scaffolding production-ready Rust applications. Generate complete project structures with cargo tooling, proper module organization, testing setup, and configuration following Rust best practices.

## Context

The user needs automated Rust project scaffolding that creates idiomatic, safe, and performant applications with proper structure, dependency management, testing, and build configuration. Focus on Rust idioms and scalable architecture.

## Requirements

$ARGUMENTS

## Instructions

### 1. Analyze Project Type

Determine the project type from user requirements:

- **Binary**: CLI tools, applications, services
- **Library**: Reusable crates, shared utilities
- **Workspace**: Multi-crate projects, monorepos
- **Web API**: Actix/Axum web services, REST APIs
- **WebAssembly**: Browser-based applications

### 2. Initialize Project with Cargo

```bash
# Create binary project
cargo new project-name
cd project-name

# Or create library
cargo new --lib library-name

# Initialize git (cargo does this automatically)
# Add to .gitignore if needed
echo "/target" >> .gitignore
echo "Cargo.lock" >> .gitignore  # For libraries only
```

### 3. Generate Binary Project Structure

```
binary-project/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── cli.rs
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── init.rs
│   │   └── run.rs
│   ├── error.rs
│   └── lib.rs
├── tests/
│   ├── integration_test.rs
│   └── common/
│       └── mod.rs
├── benches/
│   └── benchmark.rs
└── examples/
    └── basic_usage.rs
```

**Cargo.toml**:

```toml
[package]
name = "project-name"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
authors = ["Your Name <email@example.com>"]
description = "Project description"
license = "MIT OR Apache-2.0"
repository = "https://github.com/user/project-name"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.36", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "benchmark"
harness = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

**src/main.rs**:

```rust
use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod error;

use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Init(args) => commands::init::execute(args).await?,
        cli::Commands::Run(args) => commands::run::execute(args).await?,
    }

    Ok(())
}
```

**src/cli.rs**:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "project-name")]
#[command(about = "Project description", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new project
    Init(InitArgs),
    /// Run the application
    Run(RunArgs),
}

#[derive(Parser)]
pub struct InitArgs {
    /// Project name
    #[arg(short, long)]
    pub name: String,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}
```

**src/error.rs**:

```rust
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    InvalidInput(String),
    IoError(std::io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            AppError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;
```

### 4. Generate Library Project Structure

```
library-name/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── core.rs
│   ├── utils.rs
│   └── error.rs
├── tests/
│   └── integration_test.rs
└── examples/
    └── basic.rs
```

**Cargo.toml for Library**:

```toml
[package]
name = "library-name"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
# Keep minimal for libraries

[dev-dependencies]
tokio-test = "0.4"

[lib]
name = "library_name"
path = "src/lib.rs"
```

**src/lib.rs**:

````rust
//! Library documentation
//!
//! # Examples
//!
//! ```
//! use library_name::core::CoreType;
//!
//! let instance = CoreType::new();
//! ```

pub mod core;
pub mod error;
pub mod utils;

pub use core::CoreType;
pub use error::{Error, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
````

### 5. Generate Workspace Structure

```
workspace/
├── Cargo.toml
├── .gitignore
├── crates/
│   ├── api/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── cli/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
└── tests/
    └── integration_test.rs
```

**Cargo.toml (workspace root)**:

```toml
[workspace]
members = [
    "crates/api",
    "crates/core",
    "crates/cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
authors = ["Your Name <email@example.com>"]
license = "MIT OR Apache-2.0"

[workspace.dependencies]
tokio = { version = "1.36", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }

[profile.release]
opt-level = 3
lto = true
```

### 6. Generate Web API Structure (Axum)

```
web-api/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── users.rs
│   │   └── health.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   └── user_handler.rs
│   ├── models/
│   │   ├── mod.rs
│   │   └── user.rs
│   ├── services/
│   │   ├── mod.rs
│   │   └── user_service.rs
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── auth.rs
│   └── error.rs
└── tests/
    └── api_tests.rs
```

**Cargo.toml for Web API**:

```toml
[package]
name = "web-api"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1.36", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sqlx = { version = "0.7", features = ["runtime-tokio-native-tls", "postgres"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

**src/main.rs (Axum)**:

```rust
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;
use std::net::SocketAddr;

mod routes;
mod handlers;
mod models;
mod services;
mod error;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/health", get(routes::health::health_check))
        .nest("/api/users", routes::users::router())
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### 7. Configure Development Tools

**Makefile**:

```makefile
.PHONY: build test lint fmt run clean bench

build:
	cargo build

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt --check

run:
	cargo run

clean:
	cargo clean

bench:
	cargo bench
```

**rustfmt.toml**:

```toml
edition = "2021"
max_width = 100
tab_spaces = 4
use_small_heuristics = "Max"
```

**clippy.toml**:

```toml
cognitive-complexity-threshold = 30
```

## Output Format

1. **Project Structure**: Complete directory tree with idiomatic Rust organization
2. **Configuration**: Cargo.toml with dependencies and build settings
3. **Entry Point**: main.rs or lib.rs with proper documentation
4. **Tests**: Unit and integration test structure
5. **Documentation**: README and code documentation
6. **Development Tools**: Makefile, clippy/rustfmt configs

Focus on creating idiomatic Rust projects with strong type safety, proper error handling, and comprehensive testing setup.


---
# FILE: skills/go-concurrency-patterns/SKILL.md
---

---
name: go-concurrency-patterns
description: Master Go concurrency with goroutines, channels, sync primitives, and context. Use when building concurrent Go applications, implementing worker pools, or debugging race conditions.
---

# Go Concurrency Patterns

Production patterns for Go concurrency including goroutines, channels, synchronization primitives, and context management.

## When to Use This Skill

- Building concurrent Go applications
- Implementing worker pools and pipelines
- Managing goroutine lifecycles
- Using channels for communication
- Debugging race conditions
- Implementing graceful shutdown

## Core Concepts

### 1. Go Concurrency Primitives

| Primitive         | Purpose                          |
| ----------------- | -------------------------------- |
| `goroutine`       | Lightweight concurrent execution |
| `channel`         | Communication between goroutines |
| `select`          | Multiplex channel operations     |
| `sync.Mutex`      | Mutual exclusion                 |
| `sync.WaitGroup`  | Wait for goroutines to complete  |
| `context.Context` | Cancellation and deadlines       |

### 2. Go Concurrency Mantra

```
Don't communicate by sharing memory;
share memory by communicating.
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "sync"
    "time"
)

func main() {
    ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
    defer cancel()

    results := make(chan string, 10)
    var wg sync.WaitGroup

    // Spawn workers
    for i := 0; i < 3; i++ {
        wg.Add(1)
        go worker(ctx, i, results, &wg)
    }

    // Close results when done
    go func() {
        wg.Wait()
        close(results)
    }()

    // Collect results
    for result := range results {
        fmt.Println(result)
    }
}

func worker(ctx context.Context, id int, results chan<- string, wg *sync.WaitGroup) {
    defer wg.Done()

    select {
    case <-ctx.Done():
        return
    case results <- fmt.Sprintf("Worker %d done", id):
    }
}
```

## Patterns

### Pattern 1: Worker Pool

```go
package main

import (
    "context"
    "fmt"
    "sync"
)

type Job struct {
    ID   int
    Data string
}

type Result struct {
    JobID int
    Output string
    Err   error
}

func WorkerPool(ctx context.Context, numWorkers int, jobs <-chan Job) <-chan Result {
    results := make(chan Result, len(jobs))

    var wg sync.WaitGroup
    for i := 0; i < numWorkers; i++ {
        wg.Add(1)
        go func(workerID int) {
            defer wg.Done()
            for job := range jobs {
                select {
                case <-ctx.Done():
                    return
                default:
                    result := processJob(job)
                    results <- result
                }
            }
        }(i)
    }

    go func() {
        wg.Wait()
        close(results)
    }()

    return results
}

func processJob(job Job) Result {
    // Simulate work
    return Result{
        JobID:  job.ID,
        Output: fmt.Sprintf("Processed: %s", job.Data),
    }
}

// Usage
func main() {
    ctx, cancel := context.WithCancel(context.Background())
    defer cancel()

    jobs := make(chan Job, 100)

    // Send jobs
    go func() {
        for i := 0; i < 50; i++ {
            jobs <- Job{ID: i, Data: fmt.Sprintf("job-%d", i)}
        }
        close(jobs)
    }()

    // Process with 5 workers
    results := WorkerPool(ctx, 5, jobs)

    for result := range results {
        fmt.Printf("Result: %+v\n", result)
    }
}
```

### Pattern 2: Fan-Out/Fan-In Pipeline

```go
package main

import (
    "context"
    "sync"
)

// Stage 1: Generate numbers
func generate(ctx context.Context, nums ...int) <-chan int {
    out := make(chan int)
    go func() {
        defer close(out)
        for _, n := range nums {
            select {
            case <-ctx.Done():
                return
            case out <- n:
            }
        }
    }()
    return out
}

// Stage 2: Square numbers (can run multiple instances)
func square(ctx context.Context, in <-chan int) <-chan int {
    out := make(chan int)
    go func() {
        defer close(out)
        for n := range in {
            select {
            case <-ctx.Done():
                return
            case out <- n * n:
            }
        }
    }()
    return out
}

// Fan-in: Merge multiple channels into one
func merge(ctx context.Context, cs ...<-chan int) <-chan int {
    var wg sync.WaitGroup
    out := make(chan int)

    // Start output goroutine for each input channel
    output := func(c <-chan int) {
        defer wg.Done()
        for n := range c {
            select {
            case <-ctx.Done():
                return
            case out <- n:
            }
        }
    }

    wg.Add(len(cs))
    for _, c := range cs {
        go output(c)
    }

    // Close out after all inputs are done
    go func() {
        wg.Wait()
        close(out)
    }()

    return out
}

func main() {
    ctx, cancel := context.WithCancel(context.Background())
    defer cancel()

    // Generate input
    in := generate(ctx, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10)

    // Fan out to multiple squarers
    c1 := square(ctx, in)
    c2 := square(ctx, in)
    c3 := square(ctx, in)

    // Fan in results
    for result := range merge(ctx, c1, c2, c3) {
        fmt.Println(result)
    }
}
```

### Pattern 3: Bounded Concurrency with Semaphore

```go
package main

import (
    "context"
    "fmt"
    "golang.org/x/sync/semaphore"
    "sync"
)

type RateLimitedWorker struct {
    sem *semaphore.Weighted
}

func NewRateLimitedWorker(maxConcurrent int64) *RateLimitedWorker {
    return &RateLimitedWorker{
        sem: semaphore.NewWeighted(maxConcurrent),
    }
}

func (w *RateLimitedWorker) Do(ctx context.Context, tasks []func() error) []error {
    var (
        wg     sync.WaitGroup
        mu     sync.Mutex
        errors []error
    )

    for _, task := range tasks {
        // Acquire semaphore (blocks if at limit)
        if err := w.sem.Acquire(ctx, 1); err != nil {
            return []error{err}
        }

        wg.Add(1)
        go func(t func() error) {
            defer wg.Done()
            defer w.sem.Release(1)

            if err := t(); err != nil {
                mu.Lock()
                errors = append(errors, err)
                mu.Unlock()
            }
        }(task)
    }

    wg.Wait()
    return errors
}

// Alternative: Channel-based semaphore
type Semaphore chan struct{}

func NewSemaphore(n int) Semaphore {
    return make(chan struct{}, n)
}

func (s Semaphore) Acquire() {
    s <- struct{}{}
}

func (s Semaphore) Release() {
    <-s
}
```

### Pattern 4: Graceful Shutdown

```go
package main

import (
    "context"
    "fmt"
    "os"
    "os/signal"
    "sync"
    "syscall"
    "time"
)

type Server struct {
    shutdown chan struct{}
    wg       sync.WaitGroup
}

func NewServer() *Server {
    return &Server{
        shutdown: make(chan struct{}),
    }
}

func (s *Server) Start(ctx context.Context) {
    // Start workers
    for i := 0; i < 5; i++ {
        s.wg.Add(1)
        go s.worker(ctx, i)
    }
}

func (s *Server) worker(ctx context.Context, id int) {
    defer s.wg.Done()
    defer fmt.Printf("Worker %d stopped\n", id)

    ticker := time.NewTicker(time.Second)
    defer ticker.Stop()

    for {
        select {
        case <-ctx.Done():
            // Cleanup
            fmt.Printf("Worker %d cleaning up...\n", id)
            time.Sleep(500 * time.Millisecond) // Simulated cleanup
            return
        case <-ticker.C:
            fmt.Printf("Worker %d working...\n", id)
        }
    }
}

func (s *Server) Shutdown(timeout time.Duration) {
    // Signal shutdown
    close(s.shutdown)

    // Wait with timeout
    done := make(chan struct{})
    go func() {
        s.wg.Wait()
        close(done)
    }()

    select {
    case <-done:
        fmt.Println("Clean shutdown completed")
    case <-time.After(timeout):
        fmt.Println("Shutdown timed out, forcing exit")
    }
}

func main() {
    // Setup signal handling
    ctx, cancel := context.WithCancel(context.Background())

    sigCh := make(chan os.Signal, 1)
    signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

    server := NewServer()
    server.Start(ctx)

    // Wait for signal
    sig := <-sigCh
    fmt.Printf("\nReceived signal: %v\n", sig)

    // Cancel context to stop workers
    cancel()

    // Wait for graceful shutdown
    server.Shutdown(5 * time.Second)
}
```

### Pattern 5: Error Group with Cancellation

```go
package main

import (
    "context"
    "fmt"
    "golang.org/x/sync/errgroup"
    "net/http"
)

func fetchAllURLs(ctx context.Context, urls []string) ([]string, error) {
    g, ctx := errgroup.WithContext(ctx)

    results := make([]string, len(urls))

    for i, url := range urls {
        i, url := i, url // Capture loop variables

        g.Go(func() error {
            req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
            if err != nil {
                return fmt.Errorf("creating request for %s: %w", url, err)
            }

            resp, err := http.DefaultClient.Do(req)
            if err != nil {
                return fmt.Errorf("fetching %s: %w", url, err)
            }
            defer resp.Body.Close()

            results[i] = fmt.Sprintf("%s: %d", url, resp.StatusCode)
            return nil
        })
    }

    // Wait for all goroutines to complete or one to fail
    if err := g.Wait(); err != nil {
        return nil, err // First error cancels all others
    }

    return results, nil
}

// With concurrency limit
func fetchWithLimit(ctx context.Context, urls []string, limit int) ([]string, error) {
    g, ctx := errgroup.WithContext(ctx)
    g.SetLimit(limit) // Max concurrent goroutines

    results := make([]string, len(urls))
    var mu sync.Mutex

    for i, url := range urls {
        i, url := i, url

        g.Go(func() error {
            result, err := fetchURL(ctx, url)
            if err != nil {
                return err
            }

            mu.Lock()
            results[i] = result
            mu.Unlock()
            return nil
        })
    }

    if err := g.Wait(); err != nil {
        return nil, err
    }

    return results, nil
}
```

### Pattern 6: Concurrent Map with sync.Map

```go
package main

import (
    "sync"
)

// For frequent reads, infrequent writes
type Cache struct {
    m sync.Map
}

func (c *Cache) Get(key string) (interface{}, bool) {
    return c.m.Load(key)
}

func (c *Cache) Set(key string, value interface{}) {
    c.m.Store(key, value)
}

func (c *Cache) GetOrSet(key string, value interface{}) (interface{}, bool) {
    return c.m.LoadOrStore(key, value)
}

func (c *Cache) Delete(key string) {
    c.m.Delete(key)
}

// For write-heavy workloads, use sharded map
type ShardedMap struct {
    shards    []*shard
    numShards int
}

type shard struct {
    sync.RWMutex
    data map[string]interface{}
}

func NewShardedMap(numShards int) *ShardedMap {
    m := &ShardedMap{
        shards:    make([]*shard, numShards),
        numShards: numShards,
    }
    for i := range m.shards {
        m.shards[i] = &shard{data: make(map[string]interface{})}
    }
    return m
}

func (m *ShardedMap) getShard(key string) *shard {
    // Simple hash
    h := 0
    for _, c := range key {
        h = 31*h + int(c)
    }
    return m.shards[h%m.numShards]
}

func (m *ShardedMap) Get(key string) (interface{}, bool) {
    shard := m.getShard(key)
    shard.RLock()
    defer shard.RUnlock()
    v, ok := shard.data[key]
    return v, ok
}

func (m *ShardedMap) Set(key string, value interface{}) {
    shard := m.getShard(key)
    shard.Lock()
    defer shard.Unlock()
    shard.data[key] = value
}
```

### Pattern 7: Select with Timeout and Default

```go
func selectPatterns() {
    ch := make(chan int)

    // Timeout pattern
    select {
    case v := <-ch:
        fmt.Println("Received:", v)
    case <-time.After(time.Second):
        fmt.Println("Timeout!")
    }

    // Non-blocking send/receive
    select {
    case ch <- 42:
        fmt.Println("Sent")
    default:
        fmt.Println("Channel full, skipping")
    }

    // Priority select (check high priority first)
    highPriority := make(chan int)
    lowPriority := make(chan int)

    for {
        select {
        case msg := <-highPriority:
            fmt.Println("High priority:", msg)
        default:
            select {
            case msg := <-highPriority:
                fmt.Println("High priority:", msg)
            case msg := <-lowPriority:
                fmt.Println("Low priority:", msg)
            }
        }
    }
}
```

## Race Detection

```bash
# Run tests with race detector
go test -race ./...

# Build with race detector
go build -race .

# Run with race detector
go run -race main.go
```

## Best Practices

### Do's

- **Use context** - For cancellation and deadlines
- **Close channels** - From sender side only
- **Use errgroup** - For concurrent operations with errors
- **Buffer channels** - When you know the count
- **Prefer channels** - Over mutexes when possible

### Don'ts

- **Don't leak goroutines** - Always have exit path
- **Don't close from receiver** - Causes panic
- **Don't use shared memory** - Unless necessary
- **Don't ignore context cancellation** - Check ctx.Done()
- **Don't use time.Sleep for sync** - Use proper primitives


---
# FILE: skills/memory-safety-patterns/SKILL.md
---

---
name: memory-safety-patterns
description: Implement memory-safe programming with RAII, ownership, smart pointers, and resource management across Rust, C++, and C. Use when writing safe systems code, managing resources, or preventing memory bugs.
---

# Memory Safety Patterns

Cross-language patterns for memory-safe programming including RAII, ownership, smart pointers, and resource management.

## When to Use This Skill

- Writing memory-safe systems code
- Managing resources (files, sockets, memory)
- Preventing use-after-free and leaks
- Implementing RAII patterns
- Choosing between languages for safety
- Debugging memory issues

## Core Concepts

### 1. Memory Bug Categories

| Bug Type             | Description                      | Prevention        |
| -------------------- | -------------------------------- | ----------------- |
| **Use-after-free**   | Access freed memory              | Ownership, RAII   |
| **Double-free**      | Free same memory twice           | Smart pointers    |
| **Memory leak**      | Never free memory                | RAII, GC          |
| **Buffer overflow**  | Write past buffer end            | Bounds checking   |
| **Dangling pointer** | Pointer to freed memory          | Lifetime tracking |
| **Data race**        | Concurrent unsynchronized access | Ownership, Sync   |

### 2. Safety Spectrum

```
Manual (C) → Smart Pointers (C++) → Ownership (Rust) → GC (Go, Java)
Less safe                                              More safe
More control                                           Less control
```

## Patterns by Language

### Pattern 1: RAII in C++

```cpp
// RAII: Resource Acquisition Is Initialization
// Resource lifetime tied to object lifetime

#include <memory>
#include <fstream>
#include <mutex>

// File handle with RAII
class FileHandle {
public:
    explicit FileHandle(const std::string& path)
        : file_(path) {
        if (!file_.is_open()) {
            throw std::runtime_error("Failed to open file");
        }
    }

    // Destructor automatically closes file
    ~FileHandle() = default; // fstream closes in its destructor

    // Delete copy (prevent double-close)
    FileHandle(const FileHandle&) = delete;
    FileHandle& operator=(const FileHandle&) = delete;

    // Allow move
    FileHandle(FileHandle&&) = default;
    FileHandle& operator=(FileHandle&&) = default;

    void write(const std::string& data) {
        file_ << data;
    }

private:
    std::fstream file_;
};

// Lock guard (RAII for mutexes)
class Database {
public:
    void update(const std::string& key, const std::string& value) {
        std::lock_guard<std::mutex> lock(mutex_); // Released on scope exit
        data_[key] = value;
    }

    std::string get(const std::string& key) {
        std::shared_lock<std::shared_mutex> lock(shared_mutex_);
        return data_[key];
    }

private:
    std::mutex mutex_;
    std::shared_mutex shared_mutex_;
    std::map<std::string, std::string> data_;
};

// Transaction with rollback (RAII)
template<typename T>
class Transaction {
public:
    explicit Transaction(T& target)
        : target_(target), backup_(target), committed_(false) {}

    ~Transaction() {
        if (!committed_) {
            target_ = backup_; // Rollback
        }
    }

    void commit() { committed_ = true; }

    T& get() { return target_; }

private:
    T& target_;
    T backup_;
    bool committed_;
};
```

### Pattern 2: Smart Pointers in C++

```cpp
#include <memory>

// unique_ptr: Single ownership
class Engine {
public:
    void start() { /* ... */ }
};

class Car {
public:
    Car() : engine_(std::make_unique<Engine>()) {}

    void start() {
        engine_->start();
    }

    // Transfer ownership
    std::unique_ptr<Engine> extractEngine() {
        return std::move(engine_);
    }

private:
    std::unique_ptr<Engine> engine_;
};

// shared_ptr: Shared ownership
class Node {
public:
    std::string data;
    std::shared_ptr<Node> next;

    // Use weak_ptr to break cycles
    std::weak_ptr<Node> parent;
};

void sharedPtrExample() {
    auto node1 = std::make_shared<Node>();
    auto node2 = std::make_shared<Node>();

    node1->next = node2;
    node2->parent = node1; // Weak reference prevents cycle

    // Access weak_ptr
    if (auto parent = node2->parent.lock()) {
        // parent is valid shared_ptr
    }
}

// Custom deleter for resources
class Socket {
public:
    static void close(int* fd) {
        if (fd && *fd >= 0) {
            ::close(*fd);
            delete fd;
        }
    }
};

auto createSocket() {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    return std::unique_ptr<int, decltype(&Socket::close)>(
        new int(fd),
        &Socket::close
    );
}

// make_unique/make_shared best practices
void bestPractices() {
    // Good: Exception safe, single allocation
    auto ptr = std::make_shared<Widget>();

    // Bad: Two allocations, not exception safe
    std::shared_ptr<Widget> ptr2(new Widget());

    // For arrays
    auto arr = std::make_unique<int[]>(10);
}
```

### Pattern 3: Ownership in Rust

```rust
// Move semantics (default)
fn move_example() {
    let s1 = String::from("hello");
    let s2 = s1; // s1 is MOVED, no longer valid

    // println!("{}", s1); // Compile error!
    println!("{}", s2);
}

// Borrowing (references)
fn borrow_example() {
    let s = String::from("hello");

    // Immutable borrow (multiple allowed)
    let len = calculate_length(&s);
    println!("{} has length {}", s, len);

    // Mutable borrow (only one allowed)
    let mut s = String::from("hello");
    change(&mut s);
}

fn calculate_length(s: &String) -> usize {
    s.len()
} // s goes out of scope, but doesn't drop since borrowed

fn change(s: &mut String) {
    s.push_str(", world");
}

// Lifetimes: Compiler tracks reference validity
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Struct with references needs lifetime annotation
struct ImportantExcerpt<'a> {
    part: &'a str,
}

impl<'a> ImportantExcerpt<'a> {
    fn level(&self) -> i32 {
        3
    }

    // Lifetime elision: compiler infers 'a for &self
    fn announce_and_return_part(&self, announcement: &str) -> &str {
        println!("Attention: {}", announcement);
        self.part
    }
}

// Interior mutability
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Stats {
    count: Cell<i32>,           // Copy types
    data: RefCell<Vec<String>>, // Non-Copy types
}

impl Stats {
    fn increment(&self) {
        self.count.set(self.count.get() + 1);
    }

    fn add_data(&self, item: String) {
        self.data.borrow_mut().push(item);
    }
}

// Rc for shared ownership (single-threaded)
fn rc_example() {
    let data = Rc::new(vec![1, 2, 3]);
    let data2 = Rc::clone(&data); // Increment reference count

    println!("Count: {}", Rc::strong_count(&data)); // 2
}

// Arc for shared ownership (thread-safe)
use std::sync::Arc;
use std::thread;

fn arc_example() {
    let data = Arc::new(vec![1, 2, 3]);

    let handles: Vec<_> = (0..3)
        .map(|_| {
            let data = Arc::clone(&data);
            thread::spawn(move || {
                println!("{:?}", data);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
```

### Pattern 4: Safe Resource Management in C

```c
// C doesn't have RAII, but we can use patterns

#include <stdlib.h>
#include <stdio.h>

// Pattern: goto cleanup
int process_file(const char* path) {
    FILE* file = NULL;
    char* buffer = NULL;
    int result = -1;

    file = fopen(path, "r");
    if (!file) {
        goto cleanup;
    }

    buffer = malloc(1024);
    if (!buffer) {
        goto cleanup;
    }

    // Process file...
    result = 0;

cleanup:
    if (buffer) free(buffer);
    if (file) fclose(file);
    return result;
}

// Pattern: Opaque pointer with create/destroy
typedef struct Context Context;

Context* context_create(void);
void context_destroy(Context* ctx);
int context_process(Context* ctx, const char* data);

// Implementation
struct Context {
    int* data;
    size_t size;
    FILE* log;
};

Context* context_create(void) {
    Context* ctx = calloc(1, sizeof(Context));
    if (!ctx) return NULL;

    ctx->data = malloc(100 * sizeof(int));
    if (!ctx->data) {
        free(ctx);
        return NULL;
    }

    ctx->log = fopen("log.txt", "w");
    if (!ctx->log) {
        free(ctx->data);
        free(ctx);
        return NULL;
    }

    return ctx;
}

void context_destroy(Context* ctx) {
    if (ctx) {
        if (ctx->log) fclose(ctx->log);
        if (ctx->data) free(ctx->data);
        free(ctx);
    }
}

// Pattern: Cleanup attribute (GCC/Clang extension)
#define AUTO_FREE __attribute__((cleanup(auto_free_func)))

void auto_free_func(void** ptr) {
    free(*ptr);
}

void auto_free_example(void) {
    AUTO_FREE char* buffer = malloc(1024);
    // buffer automatically freed at end of scope
}
```

### Pattern 5: Bounds Checking

```cpp
// C++: Use containers instead of raw arrays
#include <vector>
#include <array>
#include <span>

void safe_array_access() {
    std::vector<int> vec = {1, 2, 3, 4, 5};

    // Safe: throws std::out_of_range
    try {
        int val = vec.at(10);
    } catch (const std::out_of_range& e) {
        // Handle error
    }

    // Unsafe but faster (no bounds check)
    int val = vec[2];

    // Modern C++20: std::span for array views
    std::span<int> view(vec);
    // Iterators are bounds-safe
    for (int& x : view) {
        x *= 2;
    }
}

// Fixed-size arrays
void fixed_array() {
    std::array<int, 5> arr = {1, 2, 3, 4, 5};

    // Compile-time size known
    static_assert(arr.size() == 5);

    // Safe access
    int val = arr.at(2);
}
```

```rust
// Rust: Bounds checking by default

fn rust_bounds_checking() {
    let vec = vec![1, 2, 3, 4, 5];

    // Runtime bounds check (panics if out of bounds)
    let val = vec[2];

    // Explicit option (no panic)
    match vec.get(10) {
        Some(val) => println!("Got {}", val),
        None => println!("Index out of bounds"),
    }

    // Iterators (no bounds checking needed)
    for val in &vec {
        println!("{}", val);
    }

    // Slices are bounds-checked
    let slice = &vec[1..3]; // [2, 3]
}
```

### Pattern 6: Preventing Data Races

```cpp
// C++: Thread-safe shared state
#include <mutex>
#include <shared_mutex>
#include <atomic>

class ThreadSafeCounter {
public:
    void increment() {
        // Atomic operations
        count_.fetch_add(1, std::memory_order_relaxed);
    }

    int get() const {
        return count_.load(std::memory_order_relaxed);
    }

private:
    std::atomic<int> count_{0};
};

class ThreadSafeMap {
public:
    void write(const std::string& key, int value) {
        std::unique_lock lock(mutex_);
        data_[key] = value;
    }

    std::optional<int> read(const std::string& key) {
        std::shared_lock lock(mutex_);
        auto it = data_.find(key);
        if (it != data_.end()) {
            return it->second;
        }
        return std::nullopt;
    }

private:
    mutable std::shared_mutex mutex_;
    std::map<std::string, int> data_;
};
```

```rust
// Rust: Data race prevention at compile time

use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;

// Atomic for simple types
fn atomic_example() {
    let counter = Arc::new(AtomicI32::new(0));

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Counter: {}", counter.load(Ordering::SeqCst));
}

// Mutex for complex types
fn mutex_example() {
    let data = Arc::new(Mutex::new(vec![]));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let data = Arc::clone(&data);
            thread::spawn(move || {
                let mut vec = data.lock().unwrap();
                vec.push(i);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

// RwLock for read-heavy workloads
fn rwlock_example() {
    let data = Arc::new(RwLock::new(HashMap::new()));

    // Multiple readers OK
    let read_guard = data.read().unwrap();

    // Writer blocks readers
    let write_guard = data.write().unwrap();
}
```

## Best Practices

### Do's

- **Prefer RAII** - Tie resource lifetime to scope
- **Use smart pointers** - Avoid raw pointers in C++
- **Understand ownership** - Know who owns what
- **Check bounds** - Use safe access methods
- **Use tools** - AddressSanitizer, Valgrind, Miri

### Don'ts

- **Don't use raw pointers** - Unless interfacing with C
- **Don't return local references** - Dangling pointer
- **Don't ignore compiler warnings** - They catch bugs
- **Don't use `unsafe` carelessly** - In Rust, minimize it
- **Don't assume thread safety** - Be explicit

## Debugging Tools

```bash
# AddressSanitizer (Clang/GCC)
clang++ -fsanitize=address -g source.cpp

# Valgrind
valgrind --leak-check=full ./program

# Rust Miri (undefined behavior detector)
cargo +nightly miri run

# ThreadSanitizer
clang++ -fsanitize=thread -g source.cpp
```


---
# FILE: skills/rust-async-patterns/SKILL.md
---

---
name: rust-async-patterns
description: Master Rust async programming with Tokio, async traits, error handling, and concurrent patterns. Use when building async Rust applications, implementing concurrent systems, or debugging async code.
---

# Rust Async Patterns

Production patterns for async Rust programming with Tokio runtime, including tasks, channels, streams, and error handling.

## When to Use This Skill

- Building async Rust applications
- Implementing concurrent network services
- Using Tokio for async I/O
- Handling async errors properly
- Debugging async code issues
- Optimizing async performance

## Core Concepts

### 1. Async Execution Model

```
Future (lazy) → poll() → Ready(value) | Pending
                ↑           ↓
              Waker ← Runtime schedules
```

### 2. Key Abstractions

| Concept    | Purpose                                  |
| ---------- | ---------------------------------------- |
| `Future`   | Lazy computation that may complete later |
| `async fn` | Function returning impl Future           |
| `await`    | Suspend until future completes           |
| `Task`     | Spawned future running concurrently      |
| `Runtime`  | Executor that polls futures              |

## Quick Start

```toml
# Cargo.toml
[dependencies]
tokio = { version = "1", features = ["full"] }
futures = "0.3"
async-trait = "0.1"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
```

```rust
use tokio::time::{sleep, Duration};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Async operations
    let result = fetch_data("https://api.example.com").await?;
    println!("Got: {}", result);

    Ok(())
}

async fn fetch_data(url: &str) -> Result<String> {
    // Simulated async operation
    sleep(Duration::from_millis(100)).await;
    Ok(format!("Data from {}", url))
}
```

## Patterns

### Pattern 1: Concurrent Task Execution

```rust
use tokio::task::JoinSet;
use anyhow::Result;

// Spawn multiple concurrent tasks
async fn fetch_all_concurrent(urls: Vec<String>) -> Result<Vec<String>> {
    let mut set = JoinSet::new();

    for url in urls {
        set.spawn(async move {
            fetch_data(&url).await
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(data)) => results.push(data),
            Ok(Err(e)) => tracing::error!("Task failed: {}", e),
            Err(e) => tracing::error!("Join error: {}", e),
        }
    }

    Ok(results)
}

// With concurrency limit
use futures::stream::{self, StreamExt};

async fn fetch_with_limit(urls: Vec<String>, limit: usize) -> Vec<Result<String>> {
    stream::iter(urls)
        .map(|url| async move { fetch_data(&url).await })
        .buffer_unordered(limit) // Max concurrent tasks
        .collect()
        .await
}

// Select first to complete
use tokio::select;

async fn race_requests(url1: &str, url2: &str) -> Result<String> {
    select! {
        result = fetch_data(url1) => result,
        result = fetch_data(url2) => result,
    }
}
```

### Pattern 2: Channels for Communication

```rust
use tokio::sync::{mpsc, broadcast, oneshot, watch};

// Multi-producer, single-consumer
async fn mpsc_example() {
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Spawn producer
    let tx2 = tx.clone();
    tokio::spawn(async move {
        tx2.send("Hello".to_string()).await.unwrap();
    });

    // Consume
    while let Some(msg) = rx.recv().await {
        println!("Got: {}", msg);
    }
}

// Broadcast: multi-producer, multi-consumer
async fn broadcast_example() {
    let (tx, _) = broadcast::channel::<String>(100);

    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    tx.send("Event".to_string()).unwrap();

    // Both receivers get the message
    let _ = rx1.recv().await;
    let _ = rx2.recv().await;
}

// Oneshot: single value, single use
async fn oneshot_example() -> String {
    let (tx, rx) = oneshot::channel::<String>();

    tokio::spawn(async move {
        tx.send("Result".to_string()).unwrap();
    });

    rx.await.unwrap()
}

// Watch: single producer, multi-consumer, latest value
async fn watch_example() {
    let (tx, mut rx) = watch::channel("initial".to_string());

    tokio::spawn(async move {
        loop {
            // Wait for changes
            rx.changed().await.unwrap();
            println!("New value: {}", *rx.borrow());
        }
    });

    tx.send("updated".to_string()).unwrap();
}
```

### Pattern 3: Async Error Handling

```rust
use anyhow::{Context, Result, bail};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),
}

// Using anyhow for application errors
async fn process_request(id: &str) -> Result<Response> {
    let data = fetch_data(id)
        .await
        .context("Failed to fetch data")?;

    let parsed = parse_response(&data)
        .context("Failed to parse response")?;

    Ok(parsed)
}

// Using custom errors for library code
async fn get_user(id: &str) -> Result<User, ServiceError> {
    let result = db.query(id).await?;

    match result {
        Some(user) => Ok(user),
        None => Err(ServiceError::NotFound(id.to_string())),
    }
}

// Timeout wrapper
use tokio::time::timeout;

async fn with_timeout<T, F>(duration: Duration, future: F) -> Result<T, ServiceError>
where
    F: std::future::Future<Output = Result<T, ServiceError>>,
{
    timeout(duration, future)
        .await
        .map_err(|_| ServiceError::Timeout(duration))?
}
```

### Pattern 4: Graceful Shutdown

```rust
use tokio::signal;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

async fn run_server() -> Result<()> {
    // Method 1: CancellationToken
    let token = CancellationToken::new();
    let token_clone = token.clone();

    // Spawn task that respects cancellation
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token_clone.cancelled() => {
                    tracing::info!("Task shutting down");
                    break;
                }
                _ = do_work() => {}
            }
        }
    });

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received");

    // Cancel all tasks
    token.cancel();

    // Give tasks time to cleanup
    tokio::time::sleep(Duration::from_secs(5)).await;

    Ok(())
}

// Method 2: Broadcast channel for shutdown
async fn run_with_broadcast() -> Result<()> {
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    let mut rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            _ = rx.recv() => {
                tracing::info!("Received shutdown");
            }
            _ = async { loop { do_work().await } } => {}
        }
    });

    signal::ctrl_c().await?;
    let _ = shutdown_tx.send(());

    Ok(())
}
```

### Pattern 5: Async Traits

```rust
use async_trait::async_trait;

#[async_trait]
pub trait Repository {
    async fn get(&self, id: &str) -> Result<Entity>;
    async fn save(&self, entity: &Entity) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;
}

pub struct PostgresRepository {
    pool: sqlx::PgPool,
}

#[async_trait]
impl Repository for PostgresRepository {
    async fn get(&self, id: &str) -> Result<Entity> {
        sqlx::query_as!(Entity, "SELECT * FROM entities WHERE id = $1", id)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn save(&self, entity: &Entity) -> Result<()> {
        sqlx::query!(
            "INSERT INTO entities (id, data) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET data = $2",
            entity.id,
            entity.data
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query!("DELETE FROM entities WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// Trait object usage
async fn process(repo: &dyn Repository, id: &str) -> Result<()> {
    let entity = repo.get(id).await?;
    // Process...
    repo.save(&entity).await
}
```

### Pattern 6: Streams and Async Iteration

```rust
use futures::stream::{self, Stream, StreamExt};
use async_stream::stream;

// Create stream from async iterator
fn numbers_stream() -> impl Stream<Item = i32> {
    stream! {
        for i in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            yield i;
        }
    }
}

// Process stream
async fn process_stream() {
    let stream = numbers_stream();

    // Map and filter
    let processed: Vec<_> = stream
        .filter(|n| futures::future::ready(*n % 2 == 0))
        .map(|n| n * 2)
        .collect()
        .await;

    println!("{:?}", processed);
}

// Chunked processing
async fn process_in_chunks() {
    let stream = numbers_stream();

    let mut chunks = stream.chunks(3);

    while let Some(chunk) = chunks.next().await {
        println!("Processing chunk: {:?}", chunk);
    }
}

// Merge multiple streams
async fn merge_streams() {
    let stream1 = numbers_stream();
    let stream2 = numbers_stream();

    let merged = stream::select(stream1, stream2);

    merged
        .for_each(|n| async move {
            println!("Got: {}", n);
        })
        .await;
}
```

### Pattern 7: Resource Management

```rust
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};

// Shared state with RwLock (prefer for read-heavy)
struct Cache {
    data: RwLock<HashMap<String, String>>,
}

impl Cache {
    async fn get(&self, key: &str) -> Option<String> {
        self.data.read().await.get(key).cloned()
    }

    async fn set(&self, key: String, value: String) {
        self.data.write().await.insert(key, value);
    }
}

// Connection pool with semaphore
struct Pool {
    semaphore: Semaphore,
    connections: Mutex<Vec<Connection>>,
}

impl Pool {
    fn new(size: usize) -> Self {
        Self {
            semaphore: Semaphore::new(size),
            connections: Mutex::new((0..size).map(|_| Connection::new()).collect()),
        }
    }

    async fn acquire(&self) -> PooledConnection<'_> {
        let permit = self.semaphore.acquire().await.unwrap();
        let conn = self.connections.lock().await.pop().unwrap();
        PooledConnection { pool: self, conn: Some(conn), _permit: permit }
    }
}

struct PooledConnection<'a> {
    pool: &'a Pool,
    conn: Option<Connection>,
    _permit: tokio::sync::SemaphorePermit<'a>,
}

impl Drop for PooledConnection<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let pool = self.pool;
            tokio::spawn(async move {
                pool.connections.lock().await.push(conn);
            });
        }
    }
}
```

## Debugging Tips

```rust
// Enable tokio-console for runtime debugging
// Cargo.toml: tokio = { features = ["tracing"] }
// Run: RUSTFLAGS="--cfg tokio_unstable" cargo run
// Then: tokio-console

// Instrument async functions
use tracing::instrument;

#[instrument(skip(pool))]
async fn fetch_user(pool: &PgPool, id: &str) -> Result<User> {
    tracing::debug!("Fetching user");
    // ...
}

// Track task spawning
let span = tracing::info_span!("worker", id = %worker_id);
tokio::spawn(async move {
    // Enters span when polled
}.instrument(span));
```

## Best Practices

### Do's

- **Use `tokio::select!`** - For racing futures
- **Prefer channels** - Over shared state when possible
- **Use `JoinSet`** - For managing multiple tasks
- **Instrument with tracing** - For debugging async code
- **Handle cancellation** - Check `CancellationToken`

### Don'ts

- **Don't block** - Never use `std::thread::sleep` in async
- **Don't hold locks across awaits** - Causes deadlocks
- **Don't spawn unboundedly** - Use semaphores for limits
- **Don't ignore errors** - Propagate with `?` or log
- **Don't forget Send bounds** - For spawned futures
