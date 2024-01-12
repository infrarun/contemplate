#![feature(iter_intersperse)]
#![feature(exact_size_is_empty)]

mod cli;
use cli::Cli;

mod error;
use daemonize::Daemonize;
use datasource::SourceRegistry;
use error::{Error, Result};

pub mod datasource;
pub mod filters;
pub mod plan;

pub mod reload;
use futures::FutureExt;
use reload::OnReload;

use nix::unistd::{execv, fork, ForkResult};
use std::{ffi::CString, ops::DerefMut, sync::Arc};
use tokio::sync::Mutex;

fn fork_and_exec_in_parent(path: &CString, args: &[CString]) {
    let fork = unsafe { fork() };
    let Ok(fork) = fork else {
        log::error!("Failed to fork!");
        return;
    };

    let ForkResult::Parent { child } = fork else {
        #[cfg(target_os = "linux")]
        let _ = prctl::set_death_signal(6);
        return;
    };

    log::debug!("Contemplate will continue to run as PID {child}.");

    execv(path, args).unwrap();
}

fn run_oneshot(
    plan: &mut plan::Plan,
    sources: &SourceRegistry,
    env: &mut minijinja::Environment<'_>,
    dry_run: bool,
    diff: bool,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let _guard = runtime.enter();

    let value: serde_json::Value = runtime.block_on(sources.as_figment())?.extract()?;
    plan.try_execute(env, &value, dry_run, diff)?;

    Ok(())
}

fn run_watch(
    plan: &mut plan::Plan,
    sources: &mut SourceRegistry,
    env: &mut minijinja::Environment<'_>,
    on_reload: &OnReload,
    dry_run: bool,
    diff: bool,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("contemplate-worker")
        .enable_all()
        .build()
        .map_err(|e| {
            log::error!("Could not create the tokio runtime: {e}");
            std::process::exit(1);
        })
        .unwrap();

    log::info!("Starting to watch for changes");
    let plan = Arc::new(Mutex::new(plan));
    let env = Arc::new(Mutex::new(env));
    let on_reload = Arc::new(Mutex::new(on_reload));

    let task = sources.watch(|sources| {
        let plan = plan.clone();
        let env = env.clone();
        let on_reload = on_reload.clone();
        async move {
            let Ok(value) = sources
                .as_figment()
                .await
                .unwrap()
                .extract()
                .map_err(|e| log::warn!("Error reading data: {e}. Not reloading."))
            else {
                return;
            };
            let mut plan = plan.lock().await;
            let updated_files = plan
                .execute(env.lock().await.deref_mut(), &value, dry_run, diff)
                .into_iter()
                .map(|op| op.dest.path());
            // do not fire on-reload when nothing was updated.
            if updated_files.is_empty() {
                return;
            }
            if let Err(e) = on_reload.lock().await.execute(updated_files).await {
                log::warn!("On-reload notification failed: {e:?}");
            };
        }
        .boxed()
    });

    runtime.block_on(task);
}

fn main() -> Result<()> {
    let cli = Cli::new().unwrap_or_else(|e| match e {
        Error::ClapError(e) => e.exit(),
        _ => unreachable!(),
    });

    pretty_env_logger::formatted_timed_builder()
        .filter_module("contemplate", cli.verbosity())
        .parse_env("CONTEMPLATE_LOG")
        .init();

    cli.generate_shell_completions();

    let mut sources = cli.sources();
    log::debug!("Sources: {sources:?}");
    let mut plan = cli.plan();
    log::debug!("Plan: {plan:?}");

    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
    filters::register(&mut env);
    if let Err(e) = plan.ensure_cached(&mut env) {
        log::error!("Error caching templates: {e}");
        std::process::exit(1);
    };

    log::debug!("Cached Plan: {plan:?}");

    let diff = cli.diff();
    let dry_run = cli.dry_run();

    // initial run.
    if let Err(e) = run_oneshot(&mut plan, &sources, &mut env, dry_run, diff) {
        log::error!("Error: {e}");
        std::process::exit(1);
    };

    // Watch mode, subsequent runs
    if cli.watch_mode() {
        if cli.daemonize() {
            let _ = Daemonize::new()
                .start()
                .map_err(|e| log::error!("Failed to daemonize: {e}"));
        }

        if let Some((path, args)) = cli.and_then_exec() {
            fork_and_exec_in_parent(&path, &args);
        }

        let on_reload: OnReload = cli.on_reload()?.into();
        run_watch(&mut plan, &mut sources, &mut env, &on_reload, dry_run, diff);
    } else if let Some((path, args)) = cli.and_then_exec() {
        execv(&path, &args)?;
    }

    Ok(())
}
