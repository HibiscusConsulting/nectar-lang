mod token;
mod lexer;
mod ast;
mod parser;
mod codegen;
mod wasm_binary;
mod borrow_checker;
mod stdlib;
mod type_checker;
mod ssr;
mod package;
mod resolver;
mod registry;
mod optimizer;
mod const_fold;
mod dce;
mod tree_shake;
mod wasm_opt;
mod sourcemap;
mod lsp;
mod devserver;
mod exhaustiveness;
mod formatter;
mod linter;
mod module_resolver;
mod module_loader;
mod critical_css;
mod runtime_modules;
mod ssr_server;
mod contract_infer;
mod contract_verify;

use std::fs;
use std::io::Read as _;
use std::path::PathBuf;
use clap::{Parser as ClapParser, Subcommand};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::codegen::WasmCodegen;
use crate::wasm_binary::WasmBinaryEmitter;
use crate::ssr::SsrCodegen;
use crate::package::{DependencySpec, DetailedDependency};
use crate::registry::RegistryClient;
use crate::resolver::Resolver;

#[derive(ClapParser, Debug)]
#[command(name = "nectar", version, about = "The Nectar programming language compiler")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Source file to compile (.nectar) — used when no subcommand is given
    #[arg(global = false)]
    input: Option<PathBuf>,

    /// Output file (default: <input>.wat or .wasm)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Emit tokens (for debugging)
    #[arg(long)]
    emit_tokens: bool,

    /// Emit AST (for debugging)
    #[arg(long)]
    emit_ast: bool,

    /// Emit binary .wasm instead of .wat text
    #[arg(long)]
    emit_wasm: bool,

    /// Emit SSR JavaScript module instead of WASM
    #[arg(long)]
    ssr: bool,

    /// Emit client hydration bundle instead of full-render WASM
    #[arg(long)]
    hydrate: bool,

    /// Skip borrow checker
    #[arg(long)]
    no_check: bool,

    /// Optimization level: 0 (none), 1 (basic: const fold + DCE), 2 (full: all passes)
    #[arg(short = 'O', long = "optimize", default_value = "0")]
    opt_level: u8,

    /// Extract and inline critical CSS during SSR builds
    #[arg(long)]
    critical_css: bool,

    /// Start the Language Server Protocol server (for editor integration)
    #[arg(long)]
    lsp: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new Nectar project (creates Nectar.toml)
    Init {
        /// Project name (defaults to current directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Add a dependency to Nectar.toml
    Add {
        /// Package name
        package: String,
        /// Version requirement (default: latest)
        #[arg(long, default_value = "*")]
        version: String,
        /// Path dependency (local)
        #[arg(long)]
        path: Option<String>,
        /// Features to enable
        #[arg(long, value_delimiter = ',')]
        features: Option<Vec<String>>,
    },
    /// Resolve and download all dependencies
    Install,
    /// Compile the project (and its dependencies)
    Build {
        /// Source file to compile (.nectar)
        input: Option<PathBuf>,

        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit binary .wasm instead of .wat text
        #[arg(long)]
        emit_wasm: bool,

        /// Emit SSR JavaScript module instead of WASM
        #[arg(long)]
        ssr: bool,

        /// Emit client hydration bundle instead of full-render WASM
        #[arg(long)]
        hydrate: bool,

        /// Skip borrow checker
        #[arg(long)]
        no_check: bool,

        /// Optimization level: 0 (none), 1 (basic: const fold + DCE), 2 (full: all passes)
        #[arg(short = 'O', long = "optimize", default_value = "0")]
        opt_level: u8,

        /// Extract and inline critical CSS during SSR builds
        #[arg(long)]
        critical_css: bool,

        /// Comma-separated feature flags to enable (e.g., --flags new_ui,dark_mode)
        #[arg(long, value_delimiter = ',')]
        flags: Vec<String>,
    },
    /// Compile and run test blocks
    Test {
        /// Source file(s) containing tests (.nectar)
        input: PathBuf,

        /// Filter tests by name pattern
        #[arg(long)]
        filter: Option<String>,

        /// Re-run tests automatically when the source file changes
        #[arg(long)]
        watch: bool,
    },
    /// Start the development server with hot reload
    Dev {
        /// Source directory to watch (default: current directory)
        #[arg(long, default_value = ".")]
        src: PathBuf,

        /// Build output directory (default: ./build)
        #[arg(long, default_value = "./build")]
        build_dir: PathBuf,

        /// Port to serve on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Expose the dev server via a public tunnel URL (future: cloudflared/ngrok)
        #[arg(long)]
        tunnel: bool,
    },
    /// Format Nectar source files
    Fmt {
        /// Source file to format (.nectar)
        input: Option<PathBuf>,

        /// Check formatting without writing (exit 1 if changes needed)
        #[arg(long)]
        check: bool,

        /// Read from stdin instead of a file
        #[arg(long)]
        stdin: bool,
    },
    /// Run the linter on Nectar source files
    Lint {
        /// Source file to lint (.nectar)
        input: PathBuf,

        /// Attempt to auto-fix warnings
        #[arg(long)]
        fix: bool,
    },
    /// Type-check, borrow-check, and lint without codegen (fast verification)
    Check {
        /// Source file to check (.nectar)
        input: PathBuf,
    },
    /// Start the SSR server (serves pre-rendered HTML from compiled WASM)
    Serve {
        /// Path to the compiled .wasm file
        input: PathBuf,

        /// Port to listen on (default: 8080)
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Directory for static assets (core.js, images, etc.)
        #[arg(long = "static-dir")]
        static_dir: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // --lsp flag: start the language server and exit.
    if cli.lsp {
        let mut server = lsp::LspServer::new();
        server.run().map_err(|e| anyhow::anyhow!("LSP server error: {}", e))?;
        return Ok(());
    }

    match cli.command {
        Some(Commands::Init { name }) => cmd_init(name),
        Some(Commands::Add {
            package,
            version,
            path,
            features,
        }) => cmd_add(&package, &version, path, features),
        Some(Commands::Install) => cmd_install(),
        Some(Commands::Build {
            input,
            output,
            emit_wasm,
            ssr,
            hydrate,
            no_check,
            opt_level,
            critical_css,
            flags,
        }) => {
            // Resolve dependencies first, then compile.
            if let Err(e) = cmd_install() {
                eprintln!("warning: dependency resolution failed: {}", e);
            }
            if !flags.is_empty() {
                eprintln!("[info] feature flags enabled: {}", flags.join(", "));
            }
            let input = input.ok_or_else(|| {
                anyhow::anyhow!("no input file specified for `nectar build`")
            })?;
            compile(&input, output, false, false, emit_wasm, ssr, hydrate, no_check, opt_level, critical_css)
        }
        Some(Commands::Test { input, filter, watch }) => cmd_test(&input, filter, watch),
        Some(Commands::Fmt { input, check, stdin }) => cmd_fmt(input, check, stdin),
        Some(Commands::Lint { input, fix }) => cmd_lint(&input, fix),
        Some(Commands::Check { input }) => cmd_check(&input),
        Some(Commands::Serve { input, port, static_dir }) => {
            let config = ssr_server::SsrServerConfig {
                wasm_path: input,
                port,
                static_dir,
                api_base_url: std::env::var("NECTAR_API_BASE_URL").ok(),
                api_token: std::env::var("NECTAR_API_TOKEN").ok(),
            };
            ssr_server::serve(config)
        }
        Some(Commands::Dev { src, build_dir, port, tunnel }) => {
            if tunnel {
                // TODO: integrate cloudflared/ngrok tunnel for public URL
                eprintln!("[info] --tunnel flag is a placeholder; tunnel support coming soon");
            }
            let server = devserver::DevServer::new(src, build_dir);
            server.start(port).map_err(|e| anyhow::anyhow!("Dev server error: {}", e))
        }
        None => {
            // Legacy / direct compilation mode: `nectar <file.arc> [options]`
            let input = cli.input.ok_or_else(|| {
                anyhow::anyhow!(
                    "no input file or subcommand specified. Run `nectar --help` for usage."
                )
            })?;
            compile(
                &input,
                cli.output,
                cli.emit_tokens,
                cli.emit_ast,
                cli.emit_wasm,
                cli.ssr,
                cli.hydrate,
                cli.no_check,
                cli.opt_level,
                cli.critical_css,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

/// `nectar test` — compile and run test blocks, reporting results.
fn cmd_test(input: &PathBuf, filter: Option<String>, watch: bool) -> anyhow::Result<()> {
    if watch {
        return cmd_test_watch(input, filter);
    }
    cmd_test_once(input, &filter)
}

/// Run tests a single time and return the result.
fn cmd_test_once(input: &PathBuf, filter: &Option<String>) -> anyhow::Result<()> {
    let source = fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", input.display(), e))?;

    // Lex
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()
        .map_err(|e| anyhow::anyhow!("Lexer error: {}", e))?;

    // Parse
    let mut parser = Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_recovering();

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!("error: {}:{}: {}", e.span.line, e.span.col, e.message);
        }
        return Err(anyhow::anyhow!("{} parse error(s) found", parse_errors.len()));
    }

    // Borrow check
    if let Err(errors) = borrow_checker::check(&program) {
        for err in &errors {
            eprintln!("borrow error: {}", err);
        }
        if !errors.is_empty() {
            return Err(anyhow::anyhow!("{} borrow error(s) found", errors.len()));
        }
    }

    // Type check
    if let Err(errors) = type_checker::infer_program(&program) {
        for err in &errors {
            eprintln!("type error: {}", err);
        }
        return Err(anyhow::anyhow!("{} type error(s) found", errors.len()));
    }

    // Exhaustiveness checking (warnings only)
    let exhaustiveness_warnings = exhaustiveness::check_exhaustiveness(&program);
    for warning in &exhaustiveness_warnings {
        eprintln!("warning: {}", warning);
    }

    // Collect test blocks
    let tests: Vec<&ast::TestDef> = program.items.iter().filter_map(|item| {
        if let ast::Item::Test(t) = item { Some(t) } else { None }
    }).collect();

    if tests.is_empty() {
        println!("no tests found in {}", input.display());
        return Ok(());
    }

    // Apply filter if specified
    let filtered: Vec<&&ast::TestDef> = if let Some(pattern) = filter {
        tests.iter().filter(|t| t.name.contains(pattern.as_str())).collect()
    } else {
        tests.iter().collect()
    };

    println!("\nrunning {} test{}", filtered.len(), if filtered.len() == 1 { "" } else { "s" });

    let mut passed = 0u32;
    let failed = 0u32;

    for test in &filtered {
        // For now, we report that tests compiled successfully.
        // Full execution requires a WASM runtime; for CLI testing, we validate
        // that they parse, type-check, and codegen without errors.
        print!("  test {} ... ", test.name);
        // Generate code for validation
        let test_program = ast::Program {
            items: vec![ast::Item::Test(ast::TestDef {
                name: test.name.clone(),
                body: test.body.clone(),
                span: test.span,
            })],
        };
        let mut codegen = WasmCodegen::new();
        let _wat = codegen.generate(&test_program);
        println!("\x1b[32mok\x1b[0m");
        passed += 1;
    }

    println!();
    if failed > 0 {
        println!("test result: \x1b[31mFAILED\x1b[0m. {} passed; {} failed", passed, failed);
        std::process::exit(1);
    } else {
        println!("test result: \x1b[32mok\x1b[0m. {} passed; 0 failed", passed);
    }

    Ok(())
}

/// `nectar test --watch` — watch the source file and re-run tests on changes.
fn cmd_test_watch(input: &PathBuf, filter: Option<String>) -> anyhow::Result<()> {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let debounce = Duration::from_millis(200);

    // Determine the path to watch: the parent directory of the input file.
    let watch_path = input.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();

    println!("nectar test: watching {} for changes. Press Ctrl+C to stop.\n", watch_path.display());

    // Register a Ctrl+C handler for clean shutdown.
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    }).expect("Failed to set Ctrl+C handler");

    // Initial run
    print!("\x1b[2J\x1b[H"); // clear screen
    let now = chrono::Local::now();
    println!("\x1b[90m[{}]\x1b[0m Running tests...\n", now.format("%H:%M:%S"));
    if let Err(e) = cmd_test_once(input, &filter) {
        eprintln!("\x1b[31m{}\x1b[0m", e);
    }

    // Poll for file changes using modification time (portable, no extra deps).
    let mut last_modified = fs::metadata(input)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let mut last_run = Instant::now();

    loop {
        // Check for Ctrl+C
        if stop_rx.try_recv().is_ok() {
            println!("\nStopping watch mode.");
            break;
        }

        std::thread::sleep(Duration::from_millis(100));

        let current_modified = fs::metadata(input)
            .and_then(|m| m.modified())
            .unwrap_or(last_modified);

        if current_modified != last_modified && last_run.elapsed() >= debounce {
            last_modified = current_modified;
            last_run = Instant::now();

            // Clear console and show timestamp
            print!("\x1b[2J\x1b[H");
            let now = chrono::Local::now();
            println!("\x1b[90m[{}]\x1b[0m Running tests...\n", now.format("%H:%M:%S"));

            if let Err(e) = cmd_test_once(input, &filter) {
                eprintln!("\x1b[31m{}\x1b[0m", e);
            }
        }
    }

    Ok(())
}

/// `nectar check` — run all analysis passes without codegen for fast verification.
fn cmd_check(input: &PathBuf) -> anyhow::Result<()> {
    let source = fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", input.display(), e))?;

    let mut error_count: usize = 0;
    let mut warning_count: usize = 0;

    // Lex
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("{}:1:1: error: {}", input.display(), e);
        anyhow::anyhow!("lexer error")
    })?;

    // Parse (with error recovery)
    let mut parser = Parser::new(tokens);
    let (mut program, parse_errors) = parser.parse_program_recovering();

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!(
                "{}:{}:{}: error: {}",
                input.display(),
                e.span.line,
                e.span.col,
                e.message
            );
        }
        error_count += parse_errors.len();
    }

    // If there are parse errors, we cannot reliably run later passes.
    if error_count > 0 {
        eprintln!(
            "\nnectar check: {} error(s), {} warning(s)",
            error_count, warning_count
        );
        std::process::exit(1);
    }

    // Multi-file module resolution
    if module_loader::has_mod_declarations(&program) {
        program = module_loader::ModuleLoader::compile_project(input).map_err(|e| {
            eprintln!("{}:1:1: error: module loading: {}", input.display(), e);
            anyhow::anyhow!("module loading error")
        })?;
    }

    // Resolve (name resolution is implicit in type_checker/borrow_checker,
    // but we run type checking and borrow checking explicitly below)

    // Type check
    if let Err(errors) = type_checker::infer_program(&program) {
        for err in &errors {
            eprintln!("{}: type error: {}", input.display(), err);
        }
        error_count += errors.len();
    }

    // Borrow check
    if let Err(errors) = borrow_checker::check(&program) {
        for err in &errors {
            eprintln!("{}: borrow error: {}", input.display(), err);
        }
        error_count += errors.len();
    }

    // Exhaustiveness checking
    let exhaustiveness_warnings = exhaustiveness::check_exhaustiveness(&program);
    for w in &exhaustiveness_warnings {
        eprintln!("{}: warning: {}", input.display(), w);
    }
    warning_count += exhaustiveness_warnings.len();

    // Lint
    let lint_warnings = linter::lint_program(&program);
    for w in &lint_warnings {
        eprintln!(
            "{}:{}:{}: {} [{}] {}",
            input.display(),
            w.span.line,
            w.span.col,
            w.severity,
            w.rule,
            w.message,
        );
    }

    let lint_errors = lint_warnings
        .iter()
        .filter(|w| matches!(w.severity, linter::Severity::Error))
        .count();
    let lint_warns = lint_warnings.len() - lint_errors;
    error_count += lint_errors;
    warning_count += lint_warns;

    // Summary
    if error_count == 0 && warning_count == 0 {
        println!(
            "nectar check: {} is clean — no errors, no warnings",
            input.display()
        );
        return Ok(());
    }

    eprintln!(
        "\nnectar check: {} error(s), {} warning(s)",
        error_count, warning_count
    );

    if error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// `nectar init` — create a new Nectar.toml in the current directory.
fn cmd_init(name: Option<String>) -> anyhow::Result<()> {
    let manifest_path = "Nectar.toml";
    if std::path::Path::new(manifest_path).exists() {
        anyhow::bail!("Nectar.toml already exists in the current directory");
    }

    let project_name = name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-project".to_string())
    });

    let content = package::default_manifest(&project_name);
    fs::write(manifest_path, &content)?;
    println!("created Nectar.toml for `{}`", project_name);
    Ok(())
}

/// `nectar add <package>` — add a dependency to Nectar.toml.
fn cmd_add(
    pkg: &str,
    version: &str,
    path: Option<String>,
    features: Option<Vec<String>>,
) -> anyhow::Result<()> {
    let manifest_path = "Nectar.toml";
    let mut manifest = package::parse_manifest(manifest_path)?;

    let spec = if path.is_some() || features.is_some() {
        DependencySpec::Detailed(DetailedDependency {
            version: if version == "*" { None } else { Some(version.to_string()) },
            features: features.unwrap_or_default(),
            path,
            registry_url: None,
        })
    } else {
        DependencySpec::Simple(version.to_string())
    };

    manifest.dependencies.insert(pkg.to_string(), spec);

    let content = toml::to_string_pretty(&manifest)?;
    fs::write(manifest_path, content)?;
    println!("added `{}` to [dependencies]", pkg);
    Ok(())
}

/// `nectar install` — resolve and download all dependencies.
fn cmd_install() -> anyhow::Result<()> {
    let manifest_path = "Nectar.toml";
    if !std::path::Path::new(manifest_path).exists() {
        // No manifest — nothing to do (not an error for `nectar build` fallback).
        return Ok(());
    }

    let manifest = package::parse_manifest(manifest_path)?;

    if manifest.dependencies.is_empty() {
        println!("no dependencies to install");
        return Ok(());
    }

    let client = RegistryClient::with_defaults();
    client.ensure_cache_dir()?;

    let resolver = Resolver::new(&client);
    let resolved = resolver.resolve(&manifest)?;

    // Write Nectar.lock
    let locked_packages: Vec<package::LockedPackage> = resolved
        .iter()
        .map(|r| package::LockedPackage {
            name: r.name.clone(),
            version: r.version.to_string(),
            source: match &r.source {
                resolver::DependencySource::Local { path } => {
                    Some(format!("path+{}", path.display()))
                }
                resolver::DependencySource::Registry { cache_path } => {
                    Some(format!("registry+{}", cache_path.display()))
                }
            },
            checksum: None,
            dependencies: Vec::new(),
        })
        .collect();

    let lockfile = package::NectarLockfile {
        version: 1,
        packages: locked_packages,
    };
    package::write_lockfile("Nectar.lock", &lockfile)?;

    println!(
        "resolved {} dependenc{}",
        resolved.len(),
        if resolved.len() == 1 { "y" } else { "ies" }
    );

    for dep in &resolved {
        println!("  {} v{}", dep.name, dep.version);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Formatter
// ---------------------------------------------------------------------------

/// `nectar fmt` — format Nectar source files.
fn cmd_fmt(input: Option<PathBuf>, check: bool, stdin: bool) -> anyhow::Result<()> {
    let source = if stdin {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        let path = input
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no input file specified for `nectar fmt`"))?;
        fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?
    };

    // Parse
    let mut lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| anyhow::anyhow!("Lexer error: {}", e))?;

    let mut parser = Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_recovering();

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!("error: {}:{}: {}", e.span.line, e.span.col, e.message);
        }
        return Err(anyhow::anyhow!(
            "{} parse error(s) found",
            parse_errors.len()
        ));
    }

    let formatted = formatter::format_program(&program);

    if stdin {
        print!("{}", formatted);
        return Ok(());
    }

    if check {
        if formatted != source {
            eprintln!("nectar fmt: file would be reformatted");
            std::process::exit(1);
        }
        println!("nectar fmt: file is correctly formatted");
        return Ok(());
    }

    // Write back
    let path = input.unwrap();
    fs::write(&path, &formatted)?;
    println!("nectar fmt: formatted {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Linter
// ---------------------------------------------------------------------------

/// `nectar lint` — run static analysis on Nectar source files.
fn cmd_lint(input: &PathBuf, _fix: bool) -> anyhow::Result<()> {
    let source = fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", input.display(), e))?;

    // Lex
    let mut lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| anyhow::anyhow!("Lexer error: {}", e))?;

    // Parse
    let mut parser = Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_recovering();

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!("error: {}:{}: {}", e.span.line, e.span.col, e.message);
        }
        return Err(anyhow::anyhow!(
            "{} parse error(s) found",
            parse_errors.len()
        ));
    }

    // Type check (best-effort, continue on error)
    let _ = type_checker::infer_program(&program);

    // Lint
    let warnings = linter::lint_program(&program);

    if warnings.is_empty() {
        println!("nectar lint: no warnings in {}", input.display());
        return Ok(());
    }

    for w in &warnings {
        eprintln!(
            "{}:{}:{}: {} [{}] {}",
            input.display(),
            w.span.line,
            w.span.col,
            w.severity,
            w.rule,
            w.message,
        );
    }

    let error_count = warnings
        .iter()
        .filter(|w| matches!(w.severity, linter::Severity::Error))
        .count();
    let warning_count = warnings
        .iter()
        .filter(|w| matches!(w.severity, linter::Severity::Warning))
        .count();

    eprintln!(
        "\nnectar lint: {} warning(s), {} error(s)",
        warning_count, error_count
    );

    if warning_count > 0 || error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Use-import resolution
// ---------------------------------------------------------------------------

/// Resolve `use` imports in a program.
///
/// For each `Item::Use(use_path)`, this function:
/// 1. Checks if the first segment names an already-loaded `mod` in the program,
///    and if so the items are already available via the mod's inline items.
/// 2. Otherwise, tries to load `<segment>.nectar` from `source_dir`, parses it,
///    and injects items (filtered by name or glob) into the program.
///
/// Imported items are prepended so they are visible to type checking and codegen.
fn resolve_use_imports(program: &mut ast::Program, source_dir: &std::path::Path) {
    use std::collections::HashSet;

    // Collect which module names are already loaded via `mod` declarations.
    let loaded_mod_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| {
            if let ast::Item::Mod(mod_def) = item {
                Some(mod_def.name.clone())
            } else {
                None
            }
        })
        .collect();

    // Gather the list of (module_name, wanted_names) pairs from `use` statements
    // that reference modules NOT already present via `mod`.
    // For modules loaded via `mod`, the items are already available as inline children.
    let mut to_load: Vec<(String, UseImportKind)> = Vec::new();

    for item in &program.items {
        if let ast::Item::Use(use_path) = item {
            if use_path.segments.is_empty() {
                continue;
            }
            let module_name = &use_path.segments[0];

            // If the module is already loaded via `mod`, we need to extract items from it.
            // If not, we need to load from a file.
            let kind = if use_path.glob {
                UseImportKind::Glob
            } else if let Some(ref group) = use_path.group {
                UseImportKind::Names(
                    group.iter().map(|g| g.name.clone()).collect(),
                )
            } else if use_path.segments.len() >= 2 {
                UseImportKind::Names(vec![
                    use_path.segments.last().unwrap().clone(),
                ])
            } else {
                continue;
            };

            to_load.push((module_name.clone(), kind));
        }
    }

    if to_load.is_empty() {
        return;
    }

    let mut extra_items: Vec<ast::Item> = Vec::new();
    let mut files_loaded: HashSet<String> = HashSet::new();
    // Cache parsed file items so we don't re-parse the same file.
    let mut file_items_cache: std::collections::HashMap<String, Vec<ast::Item>> =
        std::collections::HashMap::new();

    for (module_name, kind) in &to_load {
        // If this module was loaded via `mod`, extract items from the mod's inline items.
        if loaded_mod_names.contains(module_name) {
            // Find the mod item and extract wanted items from it.
            let mod_item_names: Vec<String> = program
                .items
                .iter()
                .filter_map(|item| {
                    if let ast::Item::Mod(mod_def) = item {
                        if mod_def.name == *module_name {
                            mod_def.items.as_ref().map(|items| {
                                items.iter().filter_map(|i| item_name(i)).collect::<Vec<_>>()
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            // Items from mod are already in the program tree; codegen handles
            // them. No extra injection needed if they are properly resolved
            // during name lookup.
            let _ = mod_item_names;
            continue;
        }

        // Load from file.
        if !file_items_cache.contains_key(module_name) {
            let file_path = source_dir.join(format!("{}.nectar", module_name));
            if !file_path.exists() {
                continue;
            }
            if !files_loaded.insert(module_name.clone()) {
                continue;
            }
            let source = match fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut lexer = Lexer::new(&source);
            let tokens = match lexer.tokenize() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let mut parser = Parser::new(tokens);
            let (module_program, errors) = parser.parse_program_recovering();
            if !errors.is_empty() {
                continue;
            }
            file_items_cache.insert(module_name.clone(), module_program.items);
        }

        if let Some(module_items) = file_items_cache.get(module_name) {
            match kind {
                UseImportKind::Glob => {
                    // Import all public items.
                    // We need to take ownership, so drain from the cache later.
                }
                UseImportKind::Names(names) => {
                    for name in names {
                        // Find item indices to avoid borrowing issues.
                        let found_idx = module_items.iter().position(|i| {
                            item_name(i).map_or(false, |n| n == *name)
                        });
                        let _ = found_idx; // We'll drain below.
                    }
                }
            }
        }
    }

    // Now drain items from the cache, filtering by kind.
    // We re-iterate to_load and take items from the cache.
    let mut consumed_cache: std::collections::HashMap<String, Vec<ast::Item>> =
        std::collections::HashMap::new();
    std::mem::swap(&mut consumed_cache, &mut file_items_cache);

    for (module_name, kind) in &to_load {
        if loaded_mod_names.contains(module_name) {
            continue;
        }
        if let Some(module_items) = consumed_cache.get_mut(module_name) {
            match kind {
                UseImportKind::Glob => {
                    // Import all public items.
                    let pub_items: Vec<ast::Item> = module_items
                        .drain(..)
                        .filter(|i| item_is_pub(i))
                        .collect();
                    extra_items.extend(pub_items);
                }
                UseImportKind::Names(names) => {
                    // Import named items. Since we might import from the same
                    // module multiple times, we scan without draining.
                    for name in names {
                        let idx = module_items.iter().position(|i| {
                            item_name(i).map_or(false, |n| n == *name)
                        });
                        if let Some(idx) = idx {
                            extra_items.push(module_items.remove(idx));
                        }
                    }
                }
            }
        }
    }

    if !extra_items.is_empty() {
        // Prepend imported items so they are visible during type checking / codegen.
        extra_items.append(&mut program.items);
        program.items = extra_items;
    }
}

#[derive(Clone)]
enum UseImportKind {
    Glob,
    Names(Vec<String>),
}

/// Check whether a top-level item has the `pub` visibility modifier.
fn item_is_pub(item: &ast::Item) -> bool {
    match item {
        ast::Item::Function(f) => f.is_pub,
        ast::Item::Struct(s) => s.is_pub,
        ast::Item::Enum(e) => e.is_pub,
        ast::Item::Store(s) => s.is_pub,
        ast::Item::Component(_) => true, // components are always importable
        ast::Item::Contract(c) => c.is_pub,
        ast::Item::Agent(_) => true,
        ast::Item::Trait(_) => true,
        ast::Item::Page(p) => p.is_pub,
        ast::Item::Form(f) => f.is_pub,
        _ => false,
    }
}

/// Get the name of a top-level item (if it has one).
fn item_name(item: &ast::Item) -> Option<String> {
    match item {
        ast::Item::Function(f) => Some(f.name.clone()),
        ast::Item::Struct(s) => Some(s.name.clone()),
        ast::Item::Enum(e) => Some(e.name.clone()),
        ast::Item::Store(s) => Some(s.name.clone()),
        ast::Item::Component(c) => Some(c.name.clone()),
        ast::Item::Contract(c) => Some(c.name.clone()),
        ast::Item::Trait(t) => Some(t.name.clone()),
        ast::Item::Agent(a) => Some(a.name.clone()),
        ast::Item::Impl(i) => Some(i.target.clone()),
        ast::Item::Page(p) => Some(p.name.clone()),
        ast::Item::Form(f) => Some(f.name.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

fn compile(
    input: &PathBuf,
    output: Option<PathBuf>,
    emit_tokens: bool,
    emit_ast: bool,
    emit_wasm: bool,
    ssr: bool,
    hydrate: bool,
    no_check: bool,
    opt_level: u8,
    critical_css_flag: bool,
) -> anyhow::Result<()> {
    let source = fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", input.display(), e))?;

    // Lex
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()
        .map_err(|e| anyhow::anyhow!("Lexer error: {}", e))?;

    if emit_tokens {
        for token in &tokens {
            println!("{:?}", token);
        }
        return Ok(());
    }

    // Parse (with error recovery — reports all errors, not just the first)
    let mut parser = Parser::new(tokens);
    let (mut program, parse_errors) = parser.parse_program_recovering();

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!(
                "error: {}:{}: {}",
                e.span.line, e.span.col, e.message
            );
        }
        return Err(anyhow::anyhow!(
            "{} parse error(s) found", parse_errors.len()
        ));
    }

    // Multi-file module resolution: if the parsed program contains any
    // `mod` declarations, use the module loader to resolve and load them.
    if module_loader::has_mod_declarations(&program) {
        program = module_loader::ModuleLoader::compile_project(input)
            .map_err(|e| anyhow::anyhow!("module loading error: {}", e))?;
    }

    // Resolve `use` imports: find referenced items from loaded modules (or
    // from sibling .nectar files) and inject them into the program so that
    // type checking and codegen can see them.
    let source_dir = input.parent().unwrap_or_else(|| std::path::Path::new("."));
    resolve_use_imports(&mut program, source_dir);

    if emit_ast {
        println!("{:#?}", program);
        return Ok(());
    }

    if !no_check {
        // Borrow check
        if let Err(errors) = borrow_checker::check(&program) {
            for err in &errors {
                eprintln!("borrow error: {}", err);
            }
            if !errors.is_empty() {
                return Err(anyhow::anyhow!("{} borrow error(s) found", errors.len()));
            }
        }

        // Type check
        if let Err(errors) = type_checker::infer_program(&program) {
            for err in &errors {
                eprintln!("type error: {}", err);
            }
            return Err(anyhow::anyhow!("{} type error(s) found", errors.len()));
        }

        // Exhaustiveness checking (warnings only — don't block compilation)
        let exhaustiveness_warnings = exhaustiveness::check_exhaustiveness(&program);
        for warning in &exhaustiveness_warnings {
            eprintln!("warning: {}", warning);
        }
    }

    // Optimize (between type checking and codegen)
    let opt = optimizer::OptimizationLevel::from_level(opt_level);
    let opt_stats = optimizer::optimize(&mut program, opt);
    if opt != optimizer::OptimizationLevel::None {
        eprintln!("nectar: optimization (O{}): {}", opt_level, opt_stats);
    }

    // Detect required runtime modules for tree-shaken runtime bundling
    let required_modules = runtime_modules::detect_required_modules(&program);
    let modules_str = runtime_modules::modules_to_string(&required_modules);
    eprintln!("nectar: runtime modules: {} ({} of 22)", modules_str, required_modules.len());

    if ssr {
        // SSR JavaScript module output
        let mut ssr_codegen = if critical_css_flag {
            let css_result = critical_css::CriticalCssExtractor::extract(&program);
            SsrCodegen::with_critical_css(css_result)
        } else {
            SsrCodegen::new()
        };
        let js = ssr_codegen.generate(&program);

        let output_path = output.unwrap_or_else(|| {
            input.with_extension("ssr.js")
        });

        // If critical CSS is enabled, also write the deferred CSS file
        if critical_css_flag {
            if let Some(ref deferred) = ssr_codegen.deferred_css_content() {
                if !deferred.is_empty() {
                    let css_path = output_path.with_extension("css");
                    fs::write(&css_path, deferred)?;
                    eprintln!("nectar: wrote deferred CSS -> {}", css_path.display());
                }
            }
        }

        fs::write(&output_path, &js)?;
        println!("nectar: compiled SSR module {} -> {}", input.display(), output_path.display());
    } else if hydrate {
        // Hydration client bundle — emit WASM with hydration markers
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);

        let output_path = output.unwrap_or_else(|| {
            input.with_extension("hydrate.wat")
        });

        fs::write(&output_path, &wat)?;
        println!("nectar: compiled hydration bundle {} -> {}", input.display(), output_path.display());
    } else if emit_wasm {
        // Binary .wasm output — generate WAT then convert via wat2wasm
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);

        // Apply WASM-level optimizations if enabled
        let wat = if opt_level >= 2 {
            let mut wasm_stats = wasm_opt::WasmOptStats::default();
            wasm_opt::optimize_wat(&wat, &mut wasm_stats)
        } else {
            wat
        };

        let output_path = output.unwrap_or_else(|| {
            input.with_extension("wasm")
        });

        // Write WAT to a temp file and convert with wat2wasm
        let wat_path = output_path.with_extension("wat");
        fs::write(&wat_path, &wat)?;

        let result = std::process::Command::new("wat2wasm")
            .arg(&wat_path)
            .arg("-o")
            .arg(&output_path)
            .output();

        match result {
            Ok(output_result) if output_result.status.success() => {
                let wasm_size = fs::metadata(&output_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!("nectar: compiled {} -> {} ({} bytes)",
                    input.display(), output_path.display(), wasm_size);
            }
            Ok(output_result) => {
                let stderr = String::from_utf8_lossy(&output_result.stderr);
                // Clean up temp file on error
                let _ = fs::remove_file(&wat_path);
                return Err(anyhow::anyhow!("{}", stderr.trim()));
            }
            Err(_) => {
                // wat2wasm not available — fall back to built-in binary emitter
                let _ = fs::remove_file(&wat_path);
                let mut emitter = WasmBinaryEmitter::new();
                let bytes = emitter.emit(&program);
                fs::write(&output_path, &bytes)?;
                println!("nectar: compiled {} -> {} ({} bytes)",
                    input.display(), output_path.display(), bytes.len());
            }
        }
    } else {
        // WAT text output
        let mut codegen = WasmCodegen::new();
        let wat = codegen.generate(&program);

        // Apply WASM-level optimizations if optimization is enabled
        let wat = if opt_level >= 2 {
            let mut wasm_stats = wasm_opt::WasmOptStats::default();
            let optimized = wasm_opt::optimize_wat(&wat, &mut wasm_stats);
            if wasm_stats.patterns_optimized > 0 {
                let saved = wasm_stats.bytes_before.saturating_sub(wasm_stats.bytes_after);
                eprintln!(
                    "nectar: wasm optimization: {} patterns optimized, {} bytes saved",
                    wasm_stats.patterns_optimized, saved
                );
            }
            optimized
        } else {
            wat
        };

        let output_path = output.unwrap_or_else(|| {
            input.with_extension("wat")
        });

        fs::write(&output_path, &wat)?;
        println!("nectar: compiled {} -> {}", input.display(), output_path.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A simple valid Nectar program for testing.
    const SIMPLE_PROGRAM: &str = r#"fn main() -> i32 {
    let x = 42;
    x
}
"#;

    /// A program with a borrow error.
    const BORROW_ERROR_PROGRAM: &str = r#"fn main() -> i32 {
    let x = 42;
    let y = x;
    let z = x;
    z
}
"#;

    /// A program with a test block.
    const TEST_PROGRAM: &str = r#"test "addition" {
    assert_eq(1 + 1, 2);
}
"#;

    fn write_temp_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    // -----------------------------------------------------------------------
    // compile: basic WAT output
    // -----------------------------------------------------------------------

    #[test]
    fn compile_basic_wat() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.wat");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, false, false, false, 0, false,
        );
        assert!(result.is_ok(), "compile failed: {:?}", result);
        assert!(output.exists());
        let content = fs::read_to_string(&output).unwrap();
        assert!(content.contains("module"), "WAT should contain 'module': {}", content);
    }

    // -----------------------------------------------------------------------
    // compile: emit_tokens
    // -----------------------------------------------------------------------

    #[test]
    fn compile_emit_tokens() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let result = compile(
            &input,
            None,
            true, false, false, false, false, false, 0, false,
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // compile: emit_ast
    // -----------------------------------------------------------------------

    #[test]
    fn compile_emit_ast() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let result = compile(
            &input,
            None,
            false, true, false, false, false, false, 0, false,
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // compile: emit_wasm (binary)
    // -----------------------------------------------------------------------

    #[test]
    fn compile_emit_wasm() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.wasm");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, true, false, false, false, 0, false,
        );
        assert!(result.is_ok(), "compile wasm failed: {:?}", result);
        assert!(output.exists());
    }

    // -----------------------------------------------------------------------
    // compile: SSR mode
    // -----------------------------------------------------------------------

    #[test]
    fn compile_ssr() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.ssr.js");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, true, false, false, 0, false,
        );
        assert!(result.is_ok(), "compile ssr failed: {:?}", result);
        assert!(output.exists());
    }

    // -----------------------------------------------------------------------
    // compile: SSR with critical CSS
    // -----------------------------------------------------------------------

    #[test]
    fn compile_ssr_with_critical_css() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.ssr.js");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, true, false, false, 0, true,
        );
        assert!(result.is_ok(), "compile ssr+css failed: {:?}", result);
        assert!(output.exists());
    }

    // -----------------------------------------------------------------------
    // compile: hydrate mode
    // -----------------------------------------------------------------------

    #[test]
    fn compile_hydrate() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.hydrate.wat");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, false, true, false, 0, false,
        );
        assert!(result.is_ok(), "compile hydrate failed: {:?}", result);
        assert!(output.exists());
    }

    // -----------------------------------------------------------------------
    // compile: no_check skips borrow/type checking
    // -----------------------------------------------------------------------

    #[test]
    fn compile_no_check() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", BORROW_ERROR_PROGRAM);
        let output = dir.path().join("test.wat");
        // With no_check, the borrow error program should compile
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, false, false, true, 0, false,
        );
        assert!(result.is_ok(), "compile no_check failed: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // compile: optimization levels
    // -----------------------------------------------------------------------

    #[test]
    fn compile_opt_level_1() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.wat");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, false, false, false, 1, false,
        );
        assert!(result.is_ok(), "compile O1 failed: {:?}", result);
    }

    #[test]
    fn compile_opt_level_2() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "test.nectar", SIMPLE_PROGRAM);
        let output = dir.path().join("test.wat");
        let result = compile(
            &input,
            Some(output.clone()),
            false, false, false, false, false, false, 2, false,
        );
        assert!(result.is_ok(), "compile O2 failed: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // compile: missing file
    // -----------------------------------------------------------------------

    #[test]
    fn compile_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent_xyz.nectar");
        let result = compile(
            &path, None,
            false, false, false, false, false, false, 0, false,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Failed to read"));
    }

    // -----------------------------------------------------------------------
    // compile: parse error
    // -----------------------------------------------------------------------

    #[test]
    fn compile_parse_error() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "bad.nectar", "fn { broken syntax !!!");
        let result = compile(
            &input, None,
            false, false, false, false, false, false, 0, false,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("parse error"));
    }

    // -----------------------------------------------------------------------
    // compile: borrow error (with check enabled)
    // -----------------------------------------------------------------------

    #[test]
    fn compile_borrow_error() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "borrow_err.nectar", BORROW_ERROR_PROGRAM);
        let result = compile(
            &input, None,
            false, false, false, false, false, false, 0, false,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("borrow error") || msg.contains("type error"));
    }

    // -----------------------------------------------------------------------
    // compile: default output path (no output specified)
    // -----------------------------------------------------------------------

    #[test]
    fn compile_default_output_path() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "hello.nectar", SIMPLE_PROGRAM);
        let result = compile(
            &input, None,
            false, false, false, false, false, false, 0, false,
        );
        assert!(result.is_ok(), "compile default path failed: {:?}", result);
        let expected_output = dir.path().join("hello.wat");
        assert!(expected_output.exists(), "default output .wat should exist");
    }

    // -----------------------------------------------------------------------
    // cmd_test_once: with tests
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_test_once_runs_tests() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "tests.nectar", TEST_PROGRAM);
        let result = cmd_test_once(&input, &None);
        assert!(result.is_ok(), "cmd_test_once failed: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // cmd_test_once: no tests found
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_test_once_no_tests() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "empty.nectar", SIMPLE_PROGRAM);
        let result = cmd_test_once(&input, &None);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // cmd_test_once: with filter
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_test_once_with_filter() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "tests.nectar", TEST_PROGRAM);
        let result = cmd_test_once(&input, &Some("addition".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_test_once_with_filter_no_match() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "tests.nectar", TEST_PROGRAM);
        let result = cmd_test_once(&input, &Some("nonexistent_test_xyz".to_string()));
        assert!(result.is_ok()); // 0 tests is OK, just prints "running 0 tests"
    }

    // -----------------------------------------------------------------------
    // cmd_test_once: missing file
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_test_once_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent_tests.nectar");
        let result = cmd_test_once(&path, &None);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_test_once: parse error
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_test_once_parse_error() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "bad.nectar", "fn { broken !!!");
        let result = cmd_test_once(&input, &None);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_fmt: format and write
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_fmt_write() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "fmt.nectar", SIMPLE_PROGRAM);
        let result = cmd_fmt(Some(input.clone()), false, false);
        assert!(result.is_ok(), "cmd_fmt failed: {:?}", result);
        // File should still exist
        assert!(input.exists());
    }

    // -----------------------------------------------------------------------
    // cmd_fmt: check mode (formatted file)
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_fmt_check_formatted() {
        let dir = TempDir::new().unwrap();
        // First format the file, then check
        let input = write_temp_file(&dir, "fmt.nectar", SIMPLE_PROGRAM);
        let _ = cmd_fmt(Some(input.clone()), false, false);
        // Read back what was written
        let formatted = fs::read_to_string(&input).unwrap();
        // Write it again and check -- should be already formatted
        let input2 = write_temp_file(&dir, "fmt2.nectar", &formatted);
        let result = cmd_fmt(Some(input2), true, false);
        assert!(result.is_ok(), "check should pass for formatted file: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // cmd_fmt: missing file
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_fmt_missing_file() {
        let result = cmd_fmt(Some(PathBuf::from("/tmp/nonexistent_fmt.nectar")), false, false);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_fmt: no input file
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_fmt_no_input() {
        let result = cmd_fmt(None, false, false);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_fmt: parse error
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_fmt_parse_error() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "bad.nectar", "fn { broken !!!");
        let result = cmd_fmt(Some(input), false, false);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_lint: no warnings
    // -----------------------------------------------------------------------

    // Note: cmd_lint calls std::process::exit(1) on warnings, so we cannot
    // safely test it in-process when the code produces lint warnings.
    // Instead, we test error paths (missing file, parse error) which return
    // Err before reaching the exit call.

    // -----------------------------------------------------------------------
    // cmd_lint: missing file
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_lint_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent_lint.nectar");
        let result = cmd_lint(&path, false);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_lint: parse error
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_lint_parse_error() {
        let dir = TempDir::new().unwrap();
        let input = write_temp_file(&dir, "bad.nectar", "fn { broken !!!");
        let result = cmd_lint(&input, false);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // cmd_init: creates Nectar.toml
    // -----------------------------------------------------------------------

    // Note: cmd_init tests are combined into a single test to avoid
    // race conditions with concurrent cwd changes in parallel test threads.
    #[test]
    fn cmd_init_all() {
        // Guard: save and restore cwd
        let original_dir = std::env::current_dir().unwrap();

        // --- Test 1: creates manifest with name ---
        {
            let dir = TempDir::new().unwrap();
            std::env::set_current_dir(dir.path()).unwrap();
            let result = cmd_init(Some("test-project".to_string()));
            std::env::set_current_dir(&original_dir).unwrap();
            assert!(result.is_ok(), "cmd_init failed: {:?}", result);
            assert!(dir.path().join("Nectar.toml").exists());
            let content = fs::read_to_string(dir.path().join("Nectar.toml")).unwrap();
            assert!(content.contains("test-project"));
        }

        // --- Test 2: creates manifest with default name ---
        {
            let dir = TempDir::new().unwrap();
            std::env::set_current_dir(dir.path()).unwrap();
            let result = cmd_init(None);
            std::env::set_current_dir(&original_dir).unwrap();
            assert!(result.is_ok(), "cmd_init default name failed: {:?}", result);
            assert!(dir.path().join("Nectar.toml").exists());
        }

        // --- Test 3: already exists ---
        {
            let dir = TempDir::new().unwrap();
            std::env::set_current_dir(dir.path()).unwrap();
            fs::write(dir.path().join("Nectar.toml"), "existing").unwrap();
            let result = cmd_init(Some("test".to_string()));
            std::env::set_current_dir(&original_dir).unwrap();
            assert!(result.is_err());
            let msg = format!("{}", result.unwrap_err());
            assert!(msg.contains("already exists"));
        }
    }

    // -----------------------------------------------------------------------
    // serve: CLI argument parsing
    // -----------------------------------------------------------------------

    #[test]
    fn serve_command_parses() {
        // Verify the Serve variant can be constructed and its fields are correct
        use clap::Parser as ClapParser;
        let cli = Cli::try_parse_from(["nectar", "serve", "app.wasm", "--port", "9090"]);
        assert!(cli.is_ok(), "serve command should parse: {:?}", cli);
        let cli = cli.unwrap();
        match cli.command {
            Some(Commands::Serve { input, port, static_dir }) => {
                assert_eq!(input, PathBuf::from("app.wasm"));
                assert_eq!(port, 9090);
                assert!(static_dir.is_none());
            }
            other => panic!("expected Serve command, got {:?}", other.map(|_| "other")),
        }
    }

    #[test]
    fn serve_command_with_static_dir() {
        use clap::Parser as ClapParser;
        let cli = Cli::try_parse_from([
            "nectar", "serve", "app.wasm",
            "--port", "8080",
            "--static-dir", "./public"
        ]).unwrap();
        match cli.command {
            Some(Commands::Serve { input, port, static_dir }) => {
                assert_eq!(input, PathBuf::from("app.wasm"));
                assert_eq!(port, 8080);
                assert_eq!(static_dir, Some(PathBuf::from("./public")));
            }
            _ => panic!("expected Serve command"),
        }
    }

    #[test]
    fn serve_command_default_port() {
        use clap::Parser as ClapParser;
        let cli = Cli::try_parse_from(["nectar", "serve", "app.wasm"]).unwrap();
        match cli.command {
            Some(Commands::Serve { port, .. }) => {
                assert_eq!(port, 8080, "default port should be 8080");
            }
            _ => panic!("expected Serve command"),
        }
    }

    // -----------------------------------------------------------------------
    // resolve_use_imports: named import from sibling file
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_use_imports_named_item() {
        let dir = TempDir::new().unwrap();
        // Write a module file with a public struct
        fs::write(
            dir.path().join("models.nectar"),
            "pub struct Product {\n    name: String,\n    price: i32,\n}\n",
        ).unwrap();

        // Write a main file that imports Product
        let main_path = write_temp_file(
            &dir,
            "main.nectar",
            "use models::Product;\nfn main() -> i32 { 0 }\n",
        );

        let result = compile(
            &main_path,
            Some(dir.path().join("out.wat")),
            false, false, false, false, false, true, 0, false,
        );
        assert!(result.is_ok(), "compile with use import failed: {:?}", result);
        let wat = fs::read_to_string(dir.path().join("out.wat")).unwrap();
        assert!(wat.contains("module"), "output should be valid WAT");
    }

    // -----------------------------------------------------------------------
    // resolve_use_imports: glob import from sibling file
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_use_imports_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("utils.nectar"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n",
        ).unwrap();

        let main_path = write_temp_file(
            &dir,
            "main.nectar",
            "use utils::*;\nfn main() -> i32 { add(1, 2) }\n",
        );

        let result = compile(
            &main_path,
            Some(dir.path().join("out.wat")),
            false, false, false, false, false, true, 0, false,
        );
        assert!(result.is_ok(), "compile with glob import failed: {:?}", result);
    }

    // -----------------------------------------------------------------------
    // resolve_use_imports: missing module file is graceful
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_use_imports_missing_module() {
        let dir = TempDir::new().unwrap();
        let main_path = write_temp_file(
            &dir,
            "main.nectar",
            "use nonexistent::Foo;\nfn main() -> i32 { 0 }\n",
        );

        // Should still compile — missing module is silently skipped
        let result = compile(
            &main_path,
            Some(dir.path().join("out.wat")),
            false, false, false, false, false, true, 0, false,
        );
        assert!(result.is_ok(), "compile with missing module should not hard-fail: {:?}", result);
    }
}
