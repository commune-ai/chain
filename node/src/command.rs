use futures::TryFutureExt;
// Substrate
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;

#[cfg(feature = "testnet")]
use sc_service::DatabaseSource;
// Frontier
#[cfg(feature = "testnet")]
use fc_db::kv::frontier_database_dir;

use crate::{
    chain_spec,
    cli::{Cli, Subcommand},
    service::{self, Other},
};

#[cfg(feature = "testnet")]
use crate::service::db_config_dir;

#[cfg(feature = "runtime-benchmarks")]
use crate::chain_spec::get_account_id_from_seed;

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "Commune Chain Node".into()
    }

    fn impl_version() -> String {
        env!("SUBSTRATE_CLI_IMPL_VERSION").into()
    }

    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").into()
    }

    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").into()
    }

    fn support_url() -> String {
        "https://communeai.org/".into()
    }

    fn copyright_start_year() -> i32 {
        2023
    }

    fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
        Ok(match id {
            "local" | "dev" => Box::new(chain_spec::generate_config("./specs/local.json")?),
            "test" => Box::new(chain_spec::ChainSpec::from_json_bytes(
                include_bytes!("../chain-specs/test.json").as_ref(),
            )?),
            "main" => Box::new(chain_spec::ChainSpec::from_json_bytes(
                include_bytes!("../chain-specs/main.json").as_ref(),
            )?),
            path => Box::new(
                chain_spec::ChainSpec::from_json_file(std::path::PathBuf::from(path)).or_else(
                    |_| {
                        eprintln!("failed to load {path} as a chain spec file, using as patch...");
                        chain_spec::generate_config(path)
                    },
                )?,
            ),
        })
    }
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
    let mut cli = Cli::from_args();
    cli.run.shared_params.detailed_log_output = true;
    cli.run.shared_params.log.extend([
        "info".to_string(),
        "pallet_chain=info".to_string(),
        "pallet_governance=info".to_string(),
        "pallet_emission=info".to_string(),
    ]);

    match &cli.subcommand {
        Some(Subcommand::Key(cmd)) => cmd.run(&cli),
        Some(Subcommand::BuildSpec(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
        }
        Some(Subcommand::CheckBlock(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    import_queue,
                    task_manager,
                    ..
                } = service::new_chain_ops(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                )?;

                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::ExportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    other: Other { config, .. },
                    ..
                } = service::new_chain_ops(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                )?;

                Ok((cmd.run(client, config.database), task_manager))
            })
        }
        Some(Subcommand::ExportState(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    other: Other { config, .. },
                    ..
                } = service::new_chain_ops(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                )?;

                Ok((cmd.run(client, config.chain_spec), task_manager))
            })
        }
        Some(Subcommand::ImportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    import_queue,
                    task_manager,
                    ..
                } = service::new_chain_ops(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                )?;

                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::PurgeChain(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| {
                // Remove Frontier offchain db
                #[cfg(feature = "testnet")]
                let db_config_dir = db_config_dir(&config);
                #[cfg(feature = "testnet")]
                match cli.eth.frontier_backend_type {
                    crate::eth::BackendType::KeyValue => {
                        let frontier_database_config = match config.database {
                            DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
                                path: frontier_database_dir(&db_config_dir, "db"),
                                cache_size: 0,
                            },
                            DatabaseSource::ParityDb { .. } => DatabaseSource::ParityDb {
                                path: frontier_database_dir(&db_config_dir, "paritydb"),
                            },
                            _ => {
                                return Err(format!(
                                    "Cannot purge `{:?}` database",
                                    config.database
                                )
                                .into())
                            }
                        };
                        cmd.run(frontier_database_config)?;
                    }
                    #[cfg(feature = "testnet")]
                    crate::eth::BackendType::Sql => {
                        let db_path = db_config_dir.join("sql");
                        match std::fs::remove_dir_all(&db_path) {
                            Ok(_) => {
                                println!("{:?} removed.", &db_path);
                            }
                            Err(ref err) if err.kind() == std::io::ErrorKind::NotFound => {
                                eprintln!("{:?} did not exist.", &db_path);
                            }
                            Err(err) => {
                                return Err(format!(
                                    "Cannot purge `{:?}` database: {:?}",
                                    db_path, err,
                                )
                                .into())
                            }
                        };
                    }
                };
                cmd.run(config.database)
            })
        }
        Some(Subcommand::Revert(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    backend,
                    task_manager,
                    ..
                } = service::new_chain_ops(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                )?;

                let aux_revert = Box::new(move |client, _, blocks| {
                    sc_consensus_grandpa::revert(client, blocks)?;
                    Ok(())
                });

                Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
            })
        }
        #[cfg(feature = "runtime-benchmarks")]
        Some(Subcommand::Benchmark(cmd)) => {
            use crate::benchmarking::{
                inherent_benchmark_data, RemarkBuilder, TransferKeepAliveBuilder,
            };
            use frame_benchmarking_cli::{
                BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE,
            };
            use node_chain_runtime::{Hashing, EXISTENTIAL_DEPOSIT};

            let runner = cli.create_runner(cmd)?;
            match cmd {
                BenchmarkCmd::Pallet(cmd) => runner.sync_run(|config| {
                    cmd.run_with_spec::<Hashing, crate::client::HostFunctions>(Some(
                        config.chain_spec,
                    ))
                }),
                BenchmarkCmd::Block(cmd) => runner.sync_run(|mut config| {
                    let PartialComponents { client, .. } = service::new_chain_ops(config)?;
                    cmd.run(client)
                }),
                BenchmarkCmd::Storage(cmd) => runner.sync_run(|mut config| {
                    let PartialComponents {
                        client,
                        backend,
                        other: Other { config, .. },
                        ..
                    } = service::new_chain_ops(config)?;

                    let db = backend.expose_db();
                    let storage = backend.expose_storage();
                    cmd.run(config, client, db, storage)
                }),
                BenchmarkCmd::Overhead(cmd) => runner.sync_run(|mut config| {
                    let PartialComponents {
                        client,
                        other: Other { config, .. },
                        ..
                    } = service::new_chain_ops(config)?;

                    let ext_builder = RemarkBuilder::new(client.clone());
                    cmd.run(
                        config,
                        client,
                        inherent_benchmark_data()?,
                        Vec::new(),
                        &ext_builder,
                    )
                }),
                BenchmarkCmd::Extrinsic(cmd) => runner.sync_run(|mut config| {
                    let PartialComponents { client, .. } = service::new_chain_ops(config)?;

                    // Register the *Remark* and *TKA* builders.
                    let ext_factory = ExtrinsicFactory(vec![
                        Box::new(RemarkBuilder::new(client.clone())),
                        Box::new(TransferKeepAliveBuilder::new(
                            client.clone(),
                            get_account_id_from_seed::<sp_core::ecdsa::Public>("Alice"),
                            EXISTENTIAL_DEPOSIT,
                        )),
                    ]);

                    cmd.run(client, inherent_benchmark_data()?, Vec::new(), &ext_factory)
                }),
                BenchmarkCmd::Machine(cmd) => {
                    runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone()))
                }
            }
        }
        #[cfg(not(feature = "runtime-benchmarks"))]
        Some(Subcommand::Benchmark) => Err("Benchmarking wasn't enabled when building the node. \
			You can enable it with `--features runtime-benchmarks`."
            .into()),
        #[cfg(feature = "testnet")]
        Some(Subcommand::FrontierDb(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| {
                let PartialComponents {
                    client,
                    other:
                        Other {
                            frontier_backend, ..
                        },
                    ..
                } = service::new_chain_ops(config, cli.eth)?;

                let frontier_backend = match frontier_backend {
                    fc_db::Backend::KeyValue(kv) => kv,
                    _ => panic!("Only fc_db::Backend::KeyValue supported"),
                };
                #[cfg(feature = "testnet")]
                cmd.run(client, frontier_backend)
            })
        }
        _ => {
            let runner = cli.create_runner(&cli.run)?;
            runner.run_node_until_exit(|config| async move {
                service::build_full(
                    config,
                    #[cfg(feature = "testnet")]
                    cli.eth,
                    cli.sealing,
                    cli.rsa_path,
                )
                .map_err(Into::into)
                .await
            })
        }
    }
}
