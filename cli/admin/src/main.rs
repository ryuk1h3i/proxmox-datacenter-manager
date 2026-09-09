use core::matches;

use anyhow::{Context, Error};

use proxmox_router::cli::{CliCommandMap, CliEnvironment, run_async_cli_command};
use proxmox_sys::fs::CreateOptions;

mod acme;
mod cert;
mod remotes;

async fn run() -> Result<(), Error> {
    let api_user = pdm_config::api_user().context("could not get api user")?;
    let priv_user = pdm_config::priv_user().context("could not get privileged user")?;

    proxmox_product_config::init(api_user.clone(), priv_user);
    proxmox_access_control::init::init(
        &pdm_api_types::AccessControlConfig,
        pdm_buildcfg::configdir!("/access"),
    )
    .context("failed to setup access control config")?;
    proxmox_acme_api::init(pdm_buildcfg::configdir!("/acme"), false)
        .context("failed to initialize acme config")?;

    proxmox_log::Logger::from_env("PDM_LOG", proxmox_log::LevelFilter::INFO)
        .tasklog_pbs()
        .stderr()
        .init()
        .context("failed to set up logger")?;

    server::context::init().context("could not set up server context")?;

    let cmd_def = CliCommandMap::new()
        .insert("acme", acme::acme_mgmt_cli())
        .insert("cert", cert::cert_mgmt_cli())
        .insert("remote", remotes::cli());

    let args: Vec<String> = std::env::args().collect();
    let avoid_init = matches!(
        args.get(1).map(String::as_str),
        Some("bashcomplete") | Some("printdoc")
    );

    if !avoid_init {
        let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
        proxmox_rest_server::init_worker_tasks(pdm_buildcfg::PDM_LOG_DIR_M!().into(), file_opts)
            .context("failed to initialize worker tasks")?;

        let mut command_sock = proxmox_daemon::command_socket::CommandSocket::new(api_user.gid);
        proxmox_rest_server::register_task_control_commands(&mut command_sock)
            .context("failed to register task control commands")?;
        command_sock
            .spawn(proxmox_rest_server::last_worker_future())
            .context("failed to activate the socket")?;
    }

    let mut rpcenv = CliEnvironment::new();
    rpcenv.set_auth_id(Some("admin@pdm".into()));

    run_async_cli_command(cmd_def, rpcenv).await;

    Ok(())
}

fn main() -> Result<(), Error> {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_async::runtime::main(run())
}
