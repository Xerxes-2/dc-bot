use crate::error::BotError;
use poise::command;
use snafu::whatever;
use sysinfo::System;
use tracing::{error, info};

pub type Context<'a> = poise::Context<'a, (), BotError>;

#[command(
    slash_command,
    global_cooldown = 10,
    name_localized("zh-CN", "健康状态"),
    description_localized("zh-CN", "获取机器的健康状态，包括 CPU 和内存使用情况")
)]
/// Fetches the health status of machine, including CPU and memory usage.
async fn health(ctx: Context<'_>) -> Result<(), BotError> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let cpu_usage = sys.global_cpu_usage();
    let total_memory = sys.total_memory() / 1024 / 1024; // Convert to MB
    let used_memory = sys.used_memory() / 1024 / 1024; // Convert to MB
    let memory_usage = (used_memory as f64 / total_memory as f64) * 100.0;
    let message = format!(
        "CPU Usage: {:.2}%\nMemory Usage: {:.2}%\nUsed Memory: {} MB\nTotal Memory: {} MB",
        cpu_usage, memory_usage, used_memory, total_memory
    );
    ctx.say(message).await?;
    Ok(())
}

#[command(
    slash_command,
    global_cooldown = 10,
    name_localized("zh-CN", "systemd状态"),
    description_localized("zh-CN", "获取 dc-bot.service 的 systemd 状态")
)]
/// Fetches the systemd status of the `dc-bot.service`.
async fn systemd_status(ctx: Context<'_>) -> Result<(), BotError> {
    // call systemctl status command
    use std::process::Command;
    let output = Command::new("systemctl")
        .arg("status")
        .arg("dc-bot.service")
        .output()?;
    if !output.status.success() {
        error!(
            "Failed to get systemd status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        whatever!("Failed to get systemd status");
    }
    let status = String::from_utf8_lossy(&output.stdout);
    ctx.say(format!("Systemd Status:\n```\n{}\n```", status))
        .await?;
    Ok(())
}

#[command(
    slash_command,
    name_localized("zh-CN", "系统信息"),
    description_localized("zh-CN", "获取系统信息，包括系统名称、内核版本和操作系统版本")
)]
/// Fetches system information such as system name, kernel version, and OS version.
async fn sysinfo(ctx: Context<'_>) -> Result<(), BotError> {
    let sys_name = System::name().unwrap_or("Unknown".into());
    let kernel_version = System::kernel_long_version();
    let os_version = System::long_os_version().unwrap_or("Unknown".into());
    let message = format!(
        "System Name: {}\nKernel Version: {}\nOS Version: {}",
        sys_name, kernel_version, os_version
    );
    ctx.say(message).await?;
    Ok(())
}

#[command(prefix_command, owners_only)]
async fn register_health(ctx: Context<'_>) -> Result<(), BotError> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, (), BotError>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            error!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e)
            }
        }
    }
}

fn option() -> poise::FrameworkOptions<(), BotError> {
    poise::FrameworkOptions {
        commands: vec![register_health(), health(), sysinfo(), systemd_status()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: None,
            ..Default::default()
        },
        on_error: |error| {
            Box::pin(async {
                on_error(error).await;
            })
        },
        pre_command: |ctx| {
            Box::pin(async move { info!("Executing command {}", ctx.command().name) })
        },
        post_command: |ctx| {
            Box::pin(async move { info!("Finished executing command {}", ctx.command().name) })
        },
        skip_checks_for_owners: true,
        ..Default::default()
    }
}

pub fn framework() -> poise::Framework<(), BotError> {
    poise::Framework::builder()
        .setup(|_, _, _| {
            Box::pin(async move {
                // This is run when the framework is set up
                info!("Health framework has been set up!");
                Ok(())
            })
        })
        .options(option())
        .build()
}
