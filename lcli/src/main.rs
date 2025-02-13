#[macro_use]
extern crate log;
mod block_root;
mod change_genesis_time;
mod check_deposit_data;
mod create_payload_header;
mod deploy_deposit_contract;
mod eth1_genesis;
mod generate_bootnode_enr;
mod indexed_attestations;
mod insecure_validators;
mod interop_genesis;
mod mnemonic_validators;
mod mock_el;
mod new_testnet;
mod parse_ssz;
mod replace_state_pubkeys;
mod skip_slots;
mod state_root;
mod transition_blocks;

use clap::{App, Arg, ArgMatches, SubCommand};
use clap_utils::parse_optional;
use environment::{EnvironmentBuilder, LoggerConfig};
use eth2_network_config::Eth2NetworkConfig;
use parse_ssz::run_parse_ssz;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use types::{EthSpec, EthSpecId};

fn main() {
    env_logger::init();

    let matches = App::new("Lighthouse CLI Tool")
        .version(lighthouse_version::VERSION)
        .about("Performs various testing-related tasks, including defining testnets.")
        .arg(
            Arg::with_name("spec")
                .short("s")
                .long("spec")
                .value_name("STRING")
                .takes_value(true)
                .possible_values(&["minimal", "mainnet", "gnosis"])
                .default_value("mainnet")
                .global(true),
        )
        .arg(
            Arg::with_name("testnet-dir")
                .short("d")
                .long("testnet-dir")
                .value_name("PATH")
                .takes_value(true)
                .global(true)
                .help("The testnet dir."),
        )
        .arg(
            Arg::with_name("network")
                .long("network")
                .value_name("NAME")
                .takes_value(true)
                .global(true)
                .help("The network to use. Defaults to mainnet.")
                .conflicts_with("testnet-dir")
        )
        .subcommand(
            SubCommand::with_name("skip-slots")
                .about(
                    "Performs a state transition from some state across some number of skip slots",
                )
                .arg(
                    Arg::with_name("output-path")
                        .long("output-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .help("Path to output a SSZ file."),
                )
                .arg(
                    Arg::with_name("pre-state-path")
                        .long("pre-state-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .conflicts_with("beacon-url")
                        .help("Path to a SSZ file of the pre-state."),
                )
                .arg(
                    Arg::with_name("beacon-url")
                        .long("beacon-url")
                        .value_name("URL")
                        .takes_value(true)
                        .help("URL to a beacon-API provider."),
                )
                .arg(
                    Arg::with_name("state-id")
                        .long("state-id")
                        .value_name("STATE_ID")
                        .takes_value(true)
                        .requires("beacon-url")
                        .help("Identifier for a state as per beacon-API standards (slot, root, etc.)"),
                )
                .arg(
                    Arg::with_name("runs")
                        .long("runs")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .default_value("1")
                        .help("Number of repeat runs, useful for benchmarking."),
                )
                .arg(
                    Arg::with_name("state-root")
                        .long("state-root")
                        .value_name("HASH256")
                        .takes_value(true)
                        .help("Tree hash root of the provided state, to avoid computing it."),
                )
                .arg(
                    Arg::with_name("slots")
                        .long("slots")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("Number of slots to skip forward."),
                )
                .arg(
                    Arg::with_name("partial-state-advance")
                        .long("partial-state-advance")
                        .takes_value(false)
                        .help("If present, don't compute state roots when skipping forward."),
                )
        )
        .subcommand(
            SubCommand::with_name("transition-blocks")
                .about("Performs a state transition given a pre-state and block")
                .arg(
                    Arg::with_name("pre-state-path")
                        .long("pre-state-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .conflicts_with("beacon-url")
                        .requires("block-path")
                        .help("Path to load a BeaconState from as SSZ."),
                )
                .arg(
                    Arg::with_name("block-path")
                        .long("block-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .conflicts_with("beacon-url")
                        .requires("pre-state-path")
                        .help("Path to load a SignedBeaconBlock from as SSZ."),
                )
                .arg(
                    Arg::with_name("post-state-output-path")
                        .long("post-state-output-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .help("Path to output the post-state."),
                )
                .arg(
                    Arg::with_name("pre-state-output-path")
                        .long("pre-state-output-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .help("Path to output the pre-state, useful when used with --beacon-url."),
                )
                .arg(
                    Arg::with_name("block-output-path")
                        .long("block-output-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .help("Path to output the block, useful when used with --beacon-url."),
                )
                .arg(
                    Arg::with_name("beacon-url")
                        .long("beacon-url")
                        .value_name("URL")
                        .takes_value(true)
                        .help("URL to a beacon-API provider."),
                )
                .arg(
                    Arg::with_name("block-id")
                        .long("block-id")
                        .value_name("BLOCK_ID")
                        .takes_value(true)
                        .requires("beacon-url")
                        .help("Identifier for a block as per beacon-API standards (slot, root, etc.)"),
                )
                .arg(
                    Arg::with_name("runs")
                        .long("runs")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .default_value("1")
                        .help("Number of repeat runs, useful for benchmarking."),
                )
                .arg(
                    Arg::with_name("no-signature-verification")
                        .long("no-signature-verification")
                        .takes_value(false)
                        .help("Disable signature verification.")
                )
                .arg(
                    Arg::with_name("exclude-cache-builds")
                        .long("exclude-cache-builds")
                        .takes_value(false)
                        .help("If present, pre-build the committee and tree-hash caches without \
                            including them in the timings."),
                )
                .arg(
                    Arg::with_name("exclude-post-block-thc")
                        .long("exclude-post-block-thc")
                        .takes_value(false)
                        .help("If present, don't rebuild the tree-hash-cache after applying \
                            the block."),
                )
        )
        .subcommand(
            SubCommand::with_name("pretty-ssz")
                .about("Parses SSZ-encoded data from a file")
                .arg(
                    Arg::with_name("format")
                        .short("f")
                        .long("format")
                        .value_name("FORMAT")
                        .takes_value(true)
                        .required(true)
                        .default_value("json")
                        .possible_values(&["json", "yaml"])
                        .help("Output format to use")
                )
                .arg(
                    Arg::with_name("type")
                        .value_name("TYPE")
                        .takes_value(true)
                        .required(true)
                        .help("Type to decode"),
                )
                .arg(
                    Arg::with_name("ssz-file")
                        .value_name("FILE")
                        .takes_value(true)
                        .required(true)
                        .help("Path to SSZ bytes"),
                )
        )
        .subcommand(
            SubCommand::with_name("deploy-deposit-contract")
                .about(
                    "Deploy a testing eth1 deposit contract.",
                )
                .arg(
                    Arg::with_name("eth1-http")
                        .long("eth1-http")
                        .short("e")
                        .value_name("ETH1_HTTP_PATH")
                        .help("Path to an Eth1 JSON-RPC IPC endpoint")
                        .takes_value(true)
                        .required(true)
                )
                .arg(
                    Arg::with_name("confirmations")
                        .value_name("INTEGER")
                        .long("confirmations")
                        .takes_value(true)
                        .default_value("3")
                        .help("The number of block confirmations before declaring the contract deployed."),
                )
                .arg(
                    Arg::with_name("validator-count")
                        .value_name("VALIDATOR_COUNT")
                        .long("validator-count")
                        .takes_value(true)
                        .help("If present, makes `validator_count` number of INSECURE deterministic deposits after \
                                deploying the deposit contract."
                        ),
                )
        )
        .subcommand(
            SubCommand::with_name("eth1-genesis")
                .about("Listens to the eth1 chain and finds the genesis beacon state")
                .arg(
                    Arg::with_name("eth1-endpoint")
                        .short("e")
                        .long("eth1-endpoint")
                        .value_name("HTTP_SERVER")
                        .takes_value(true)
                        .help("Deprecated. Use --eth1-endpoints."),
                )
                .arg(
                    Arg::with_name("eth1-endpoints")
                        .long("eth1-endpoints")
                        .value_name("HTTP_SERVER_LIST")
                        .takes_value(true)
                        .conflicts_with("eth1-endpoint")
                        .help(
                            "One or more comma-delimited URLs to eth1 JSON-RPC http APIs. \
                                If multiple endpoints are given the endpoints are used as \
                                fallback in the given order.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("interop-genesis")
                .about("Produces an interop-compatible genesis state using deterministic keypairs")
                .arg(
                    Arg::with_name("validator-count")
                        .long("validator-count")
                        .index(1)
                        .value_name("INTEGER")
                        .takes_value(true)
                        .default_value("1024")
                        .help("The number of validators in the genesis state."),
                )
                .arg(
                    Arg::with_name("genesis-time")
                        .long("genesis-time")
                        .short("t")
                        .value_name("UNIX_EPOCH")
                        .takes_value(true)
                        .help("The value for state.genesis_time. Defaults to now."),
                )
                .arg(
                    Arg::with_name("genesis-fork-version")
                        .long("genesis-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("change-genesis-time")
                .about(
                    "Loads a file with an SSZ-encoded BeaconState and modifies the genesis time.",
                )
                .arg(
                    Arg::with_name("ssz-state")
                        .index(1)
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("The path to the SSZ file"),
                )
                .arg(
                    Arg::with_name("genesis-time")
                        .index(2)
                        .value_name("UNIX_EPOCH")
                        .takes_value(true)
                        .required(true)
                        .help("The value for state.genesis_time."),
                ),
        )
        .subcommand(
            SubCommand::with_name("replace-state-pubkeys")
                .about(
                    "Loads a file with an SSZ-encoded BeaconState and replaces \
                    all the validator pubkeys with ones derived from the mnemonic \
                    such that validator indices correspond to EIP-2334 voting keypair \
                    derivation paths.",
                )
                .arg(
                    Arg::with_name("ssz-state")
                        .index(1)
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("The path to the SSZ file"),
                )
                .arg(
                    Arg::with_name("mnemonic")
                        .index(2)
                        .value_name("BIP39_MNENMONIC")
                        .takes_value(true)
                        .default_value(
                            "replace nephew blur decorate waste convince soup column \
                            orient excite play baby",
                        )
                        .help("The mnemonic for key derivation."),
                ),
        )
        .subcommand(
            SubCommand::with_name("create-payload-header")
                .about("Generates an SSZ file containing bytes for an `ExecutionPayloadHeader`. \
                Useful as input for `lcli new-testnet --execution-payload-header FILE`. If `--fork` \
                is not provided, a payload header for the `Bellatrix` fork will be created.")
                .arg(
                    Arg::with_name("execution-block-hash")
                        .long("execution-block-hash")
                        .value_name("BLOCK_HASH")
                        .takes_value(true)
                        .help("The block hash used when generating an execution payload. This \
                            value is used for `execution_payload_header.block_hash` as well as \
                            `execution_payload_header.random`")
                        .default_value(
                            "0x0000000000000000000000000000000000000000000000000000000000000000",
                        ),
                )
                .arg(
                    Arg::with_name("genesis-time")
                        .long("genesis-time")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The genesis time when generating an execution payload.")
                )
                .arg(
                    Arg::with_name("base-fee-per-gas")
                        .long("base-fee-per-gas")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The base fee per gas field in the execution payload generated.")
                        .default_value("1000000000"),
                )
                .arg(
                    Arg::with_name("gas-limit")
                        .long("gas-limit")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The gas limit field in the execution payload generated.")
                        .default_value("30000000"),
                )
                .arg(
                    Arg::with_name("file")
                        .long("file")
                        .value_name("FILE")
                        .takes_value(true)
                        .required(true)
                        .help("Output file"),
                ).arg(
                Arg::with_name("fork")
                    .long("fork")
                    .value_name("FORK")
                    .takes_value(true)
                    .default_value("bellatrix")
                    .help("The fork for which the execution payload header should be created.")
                    .possible_values(&["merge", "bellatrix", "capella", "deneb"])
            )
        )
        .subcommand(
            SubCommand::with_name("new-testnet")
                .about(
                    "Produce a new testnet directory. If any of the optional flags are not
                    supplied the values will remain the default for the --spec flag",
                )
                .arg(
                    Arg::with_name("force")
                        .long("force")
                        .short("f")
                        .takes_value(false)
                        .help("Overwrites any previous testnet configurations"),
                )
                .arg(
                    Arg::with_name("interop-genesis-state")
                        .long("interop-genesis-state")
                        .takes_value(false)
                        .help(
                            "If present, a interop-style genesis.ssz file will be generated.",
                        ),
                )
                .arg(
                    Arg::with_name("derived-genesis-state")
                        .long("derived-genesis-state")
                        .takes_value(false)
                        .help(
                            "If present, a genesis.ssz file will be generated with keys generated from a given mnemonic.",
                        ),
                )
                .arg(
                    Arg::with_name("mnemonic-phrase")
                        .long("mnemonic-phrase")
                        .value_name("MNEMONIC_PHRASE")
                        .takes_value(true)
                        .requires("derived-genesis-state")
                        .help("The mnemonic with which we generate the validator keys for a derived genesis state"),
                )
                .arg(
                    Arg::with_name("min-genesis-time")
                        .long("min-genesis-time")
                        .value_name("UNIX_SECONDS")
                        .takes_value(true)
                        .help(
                            "The minimum permitted genesis time. For non-eth1 testnets will be
                              the genesis time. Defaults to now.",
                        ),
                )
                .arg(
                    Arg::with_name("min-genesis-active-validator-count")
                        .long("min-genesis-active-validator-count")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The number of validators required to trigger eth2 genesis."),
                )
                .arg(
                    Arg::with_name("genesis-delay")
                        .long("genesis-delay")
                        .value_name("SECONDS")
                        .takes_value(true)
                        .help("The delay between sufficient eth1 deposits and eth2 genesis."),
                )
                .arg(
                    Arg::with_name("min-deposit-amount")
                        .long("min-deposit-amount")
                        .value_name("GWEI")
                        .takes_value(true)
                        .help("The minimum permitted deposit amount."),
                )
                .arg(
                    Arg::with_name("max-effective-balance")
                        .long("max-effective-balance")
                        .value_name("GWEI")
                        .takes_value(true)
                        .help("The amount required to become a validator."),
                )
                .arg(
                    Arg::with_name("effective-balance-increment")
                        .long("effective-balance-increment")
                        .value_name("GWEI")
                        .takes_value(true)
                        .help("The steps in effective balance calculation."),
                )
                .arg(
                    Arg::with_name("ejection-balance")
                        .long("ejection-balance")
                        .value_name("GWEI")
                        .takes_value(true)
                        .help("The balance at which a validator gets ejected."),
                )
                .arg(
                    Arg::with_name("eth1-follow-distance")
                        .long("eth1-follow-distance")
                        .value_name("ETH1_BLOCKS")
                        .takes_value(true)
                        .help("The distance to follow behind the eth1 chain head."),
                )
                .arg(
                    Arg::with_name("genesis-fork-version")
                        .long("genesis-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                )
                .arg(
                    Arg::with_name("altair-fork-version")
                        .long("altair-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                )
                .arg(
                    Arg::with_name("bellatrix-fork-version")
                        .long("bellatrix-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                )
                .arg(
                    Arg::with_name("capella-fork-version")
                        .long("capella-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                )
                .arg(
                    Arg::with_name("deneb-fork-version")
                        .long("deneb-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                )
                .arg(
                    Arg::with_name("seconds-per-slot")
                        .long("seconds-per-slot")
                        .value_name("SECONDS")
                        .takes_value(true)
                        .help("Eth2 slot time"),
                )
                .arg(
                    Arg::with_name("seconds-per-eth1-block")
                        .long("seconds-per-eth1-block")
                        .value_name("SECONDS")
                        .takes_value(true)
                        .help("Eth1 block time"),
                )
                .arg(
                    Arg::with_name("eth1-id")
                        .long("eth1-id")
                        .value_name("ETH1_ID")
                        .takes_value(true)
                        .help("The chain id and network id for the eth1 testnet."),
                )
                .arg(
                    Arg::with_name("deposit-contract-address")
                        .long("deposit-contract-address")
                        .value_name("ETH1_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("The address of the deposit contract."),
                )
                .arg(
                    Arg::with_name("deposit-contract-deploy-block")
                        .long("deposit-contract-deploy-block")
                        .value_name("ETH1_BLOCK_NUMBER")
                        .takes_value(true)
                        .default_value("0")
                        .help(
                            "The block the deposit contract was deployed. Setting this is a huge
                              optimization for nodes, please do it.",
                        ),
                )
                .arg(
                    Arg::with_name("altair-fork-epoch")
                        .long("altair-fork-epoch")
                        .value_name("EPOCH")
                        .takes_value(true)
                        .help(
                            "The epoch at which to enable the Altair hard fork",
                        ),
                )
                .arg(
                    Arg::with_name("bellatrix-fork-epoch")
                        .long("bellatrix-fork-epoch")
                        .value_name("EPOCH")
                        .takes_value(true)
                        .help(
                            "The epoch at which to enable the Merge hard fork",
                        ),
                )
                .arg(
                    Arg::with_name("capella-fork-epoch")
                        .long("capella-fork-epoch")
                        .value_name("EPOCH")
                        .takes_value(true)
                        .help(
                            "The epoch at which to enable the Capella hard fork",
                        ),
                )
                .arg(
                    Arg::with_name("deneb-fork-epoch")
                        .long("deneb-fork-epoch")
                        .value_name("EPOCH")
                        .takes_value(true)
                        .help(
                            "The epoch at which to enable the deneb hard fork",
                        ),
                )
                .arg(
                    Arg::with_name("ttd")
                        .long("ttd")
                        .value_name("TTD")
                        .takes_value(true)
                        .help(
                            "The terminal total difficulty",
                        ),
                )
                .arg(
                    Arg::with_name("eth1-block-hash")
                        .long("eth1-block-hash")
                        .value_name("BLOCK_HASH")
                        .takes_value(true)
                        .help("The eth1 block hash used when generating a genesis state."),
                )
                .arg(
                    Arg::with_name("execution-payload-header")
                        .long("execution-payload-header")
                        .value_name("FILE")
                        .takes_value(true)
                        .required(false)
                        .help("Path to file containing `ExecutionPayloadHeader` SSZ bytes to be \
                            used in the genesis state."),
                )
                .arg(
                    Arg::with_name("validator-count")
                        .long("validator-count")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The number of validators when generating a genesis state."),
                )
                .arg(
                    Arg::with_name("genesis-time")
                        .long("genesis-time")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The genesis time when generating a genesis state."),
                )
                .arg(
                    Arg::with_name("proposer-score-boost")
                        .long("proposer-score-boost")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .help("The proposer score boost to apply as a percentage, e.g. 70 = 70%"),
                )
                .arg(
                    Arg::with_name("config-name")
                        .long("config-name")
                        .value_name("STRING")
                        .takes_value(true)
                        .help("Name of this config"),
                )

        )
        .subcommand(
            SubCommand::with_name("check-deposit-data")
                .about("Checks the integrity of some deposit data.")
                .arg(
                    Arg::with_name("deposit-amount")
                        .index(1)
                        .value_name("GWEI")
                        .takes_value(true)
                        .required(true)
                        .help("The amount (in Gwei) that was deposited"),
                )
                .arg(
                    Arg::with_name("deposit-data")
                        .index(2)
                        .value_name("HEX")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "A 0x-prefixed hex string of the deposit data. Should include the
                            function signature.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("generate-bootnode-enr")
                .about("Generates an ENR address to be used as a pre-genesis boot node.")
                .arg(
                    Arg::with_name("ip")
                        .long("ip")
                        .value_name("IP_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("The IP address to be included in the ENR and used for discovery"),
                )
                .arg(
                    Arg::with_name("udp-port")
                        .long("udp-port")
                        .value_name("UDP_PORT")
                        .takes_value(true)
                        .required(true)
                        .help("The UDP port to be included in the ENR and used for discovery"),
                )
                .arg(
                    Arg::with_name("tcp-port")
                        .long("tcp-port")
                        .value_name("TCP_PORT")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "The TCP port to be included in the ENR and used for application comms",
                        ),
                )
                .arg(
                    Arg::with_name("output-dir")
                        .long("output-dir")
                        .value_name("OUTPUT_DIRECTORY")
                        .takes_value(true)
                        .required(true)
                        .help("The directory in which to create the network dir"),
                )
                .arg(
                    Arg::with_name("genesis-fork-version")
                        .long("genesis-fork-version")
                        .value_name("HEX")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "Used to avoid reply attacks between testnets. Recommended to set to
                              non-default.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("insecure-validators")
                .about("Produces validator directories with INSECURE, deterministic keypairs.")
                .arg(
                    Arg::with_name("count")
                        .long("count")
                        .value_name("COUNT")
                        .takes_value(true)
                        .required(true)
                        .help("Produces validators in the range of 0..count."),
                )
                .arg(
                    Arg::with_name("base-dir")
                        .long("base-dir")
                        .value_name("BASE_DIR")
                        .takes_value(true)
                        .required(true)
                        .help("The base directory where validator keypairs and secrets are stored"),
                )
                .arg(
                    Arg::with_name("node-count")
                        .long("node-count")
                        .value_name("NODE_COUNT")
                        .takes_value(true)
                        .help("The number of nodes to divide the validator keys to"),
                )
        )
        .subcommand(
            SubCommand::with_name("mnemonic-validators")
                .about("Produces validator directories by deriving the keys from \
                        a mnemonic. For testing purposes only, DO NOT USE IN \
                        PRODUCTION!")
                .arg(
                    Arg::with_name("count")
                        .long("count")
                        .value_name("COUNT")
                        .takes_value(true)
                        .required(true)
                        .help("Produces validators in the range of 0..count."),
                )
                .arg(
                    Arg::with_name("base-dir")
                        .long("base-dir")
                        .value_name("BASE_DIR")
                        .takes_value(true)
                        .required(true)
                        .help("The base directory where validator keypairs and secrets are stored"),
                )
                .arg(
                    Arg::with_name("node-count")
                        .long("node-count")
                        .value_name("NODE_COUNT")
                        .takes_value(true)
                        .help("The number of nodes to divide the validator keys to"),
                )
                .arg(
                    Arg::with_name("mnemonic-phrase")
                        .long("mnemonic-phrase")
                        .value_name("MNEMONIC_PHRASE")
                        .takes_value(true)
                        .required(true)
                        .help("The mnemonic with which we generate the validator keys"),
                )
        )
        .subcommand(
            SubCommand::with_name("indexed-attestations")
                .about("Convert attestations to indexed form, using the committees from a state.")
                .arg(
                    Arg::with_name("state")
                        .long("state")
                        .value_name("SSZ_STATE")
                        .takes_value(true)
                        .required(true)
                        .help("BeaconState to generate committees from (SSZ)"),
                )
                .arg(
                    Arg::with_name("attestations")
                        .long("attestations")
                        .value_name("JSON_ATTESTATIONS")
                        .takes_value(true)
                        .required(true)
                        .help("List of Attestations to convert to indexed form (JSON)"),
                )
        )
        .subcommand(
            SubCommand::with_name("block-root")
                .about("Computes the block root of some block.")
                .arg(
                    Arg::with_name("block-path")
                        .long("block-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .conflicts_with("beacon-url")
                        .help("Path to load a SignedBeaconBlock from as SSZ."),
                )
                .arg(
                    Arg::with_name("beacon-url")
                        .long("beacon-url")
                        .value_name("URL")
                        .takes_value(true)
                        .help("URL to a beacon-API provider."),
                )
                .arg(
                    Arg::with_name("block-id")
                        .long("block-id")
                        .value_name("BLOCK_ID")
                        .takes_value(true)
                        .requires("beacon-url")
                        .help("Identifier for a block as per beacon-API standards (slot, root, etc.)"),
                )
                .arg(
                    Arg::with_name("runs")
                        .long("runs")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .default_value("1")
                        .help("Number of repeat runs, useful for benchmarking."),
                )
        )
        .subcommand(
            SubCommand::with_name("state-root")
                .about("Computes the state root of some state.")
                .arg(
                    Arg::with_name("state-path")
                        .long("state-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .conflicts_with("beacon-url")
                        .help("Path to load a BeaconState from as SSZ."),
                )
                .arg(
                    Arg::with_name("beacon-url")
                        .long("beacon-url")
                        .value_name("URL")
                        .takes_value(true)
                        .help("URL to a beacon-API provider."),
                )
                .arg(
                    Arg::with_name("state-id")
                        .long("state-id")
                        .value_name("BLOCK_ID")
                        .takes_value(true)
                        .requires("beacon-url")
                        .help("Identifier for a state as per beacon-API standards (slot, root, etc.)"),
                )
                .arg(
                    Arg::with_name("runs")
                        .long("runs")
                        .value_name("INTEGER")
                        .takes_value(true)
                        .default_value("1")
                        .help("Number of repeat runs, useful for benchmarking."),
                )
        )
        .subcommand(
            SubCommand::with_name("mock-el")
                .about("Creates a mock execution layer server. This is NOT SAFE and should only \
                be used for testing and development on testnets. Do not use in production. Do not \
                use on mainnet. It cannot perform validator duties.")
                .arg(
                    Arg::with_name("jwt-output-path")
                        .long("jwt-output-path")
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("Path to write the JWT secret."),
                )
                .arg(
                    Arg::with_name("listen-address")
                        .long("listen-address")
                        .value_name("IP_ADDRESS")
                        .takes_value(true)
                        .help("The server will listen on this address.")
                        .default_value("127.0.0.1")
                )
                .arg(
                    Arg::with_name("listen-port")
                        .long("listen-port")
                        .value_name("PORT")
                        .takes_value(true)
                        .help("The server will listen on this port.")
                        .default_value("8551")
                )
                .arg(
                    Arg::with_name("all-payloads-valid")
                        .long("all-payloads-valid")
                        .takes_value(true)
                        .help("Controls the response to newPayload and forkchoiceUpdated. \
                            Set to 'true' to return VALID. Set to 'false' to return SYNCING.")
                        .default_value("false")
                        .hidden(true)
                )
                .arg(
                    Arg::with_name("shanghai-time")
                        .long("shanghai-time")
                        .value_name("UNIX_TIMESTAMP")
                        .takes_value(true)
                        .help("The payload timestamp that enables Shanghai. Defaults to the mainnet value.")
                        .default_value("1681338479")
                )
                .arg(
                    Arg::with_name("cancun-time")
                        .long("cancun-time")
                        .value_name("UNIX_TIMESTAMP")
                        .takes_value(true)
                        .help("The payload timestamp that enables Cancun. No default is provided \
                                until Cancun is triggered on mainnet.")
                )
        )
        .get_matches();

    let result = matches
        .value_of("spec")
        .ok_or_else(|| "Missing --spec flag".to_string())
        .and_then(FromStr::from_str)
        .and_then(|eth_spec_id| match eth_spec_id {
            EthSpecId::Minimal => run(EnvironmentBuilder::minimal(), &matches),
            EthSpecId::Mainnet => run(EnvironmentBuilder::mainnet(), &matches),
            EthSpecId::Gnosis => run(EnvironmentBuilder::gnosis(), &matches),
        });

    match result {
        Ok(()) => process::exit(0),
        Err(e) => {
            println!("Failed to run lcli: {}", e);
            process::exit(1)
        }
    }
}

fn run<T: EthSpec>(
    env_builder: EnvironmentBuilder<T>,
    matches: &ArgMatches<'_>,
) -> Result<(), String> {
    let env = env_builder
        .multi_threaded_tokio_runtime()
        .map_err(|e| format!("should start tokio runtime: {:?}", e))?
        .initialize_logger(LoggerConfig {
            path: None,
            debug_level: String::from("trace"),
            logfile_debug_level: String::from("trace"),
            log_format: None,
            logfile_format: None,
            log_color: false,
            disable_log_timestamp: false,
            max_log_size: 0,
            max_log_number: 0,
            compression: false,
            is_restricted: true,
            sse_logging: false, // No SSE Logging in LCLI
        })
        .map_err(|e| format!("should start logger: {:?}", e))?
        .build()
        .map_err(|e| format!("should build env: {:?}", e))?;

    // Determine testnet-dir path or network name depending on CLI flags.
    let (testnet_dir, network_name) =
        if let Some(testnet_dir) = parse_optional::<PathBuf>(matches, "testnet-dir")? {
            (Some(testnet_dir), None)
        } else {
            let network_name =
                parse_optional(matches, "network")?.unwrap_or_else(|| "mainnet".to_string());
            (None, Some(network_name))
        };

    // Lazily load either the testnet dir or the network config, as required.
    // Some subcommands like new-testnet need the testnet dir but not the network config.
    let get_testnet_dir = || testnet_dir.clone().ok_or("testnet-dir is required");
    let get_network_config = || {
        if let Some(testnet_dir) = &testnet_dir {
            Eth2NetworkConfig::load(testnet_dir.clone()).map_err(|e| {
                format!(
                    "Unable to open testnet dir at {}: {}",
                    testnet_dir.display(),
                    e
                )
            })
        } else {
            let network_name = network_name.ok_or("no network name or testnet-dir provided")?;
            Eth2NetworkConfig::constant(&network_name)?.ok_or("invalid network name".into())
        }
    };

    match matches.subcommand() {
        ("transition-blocks", Some(matches)) => {
            let network_config = get_network_config()?;
            transition_blocks::run::<T>(env, network_config, matches)
                .map_err(|e| format!("Failed to transition blocks: {}", e))
        }
        ("skip-slots", Some(matches)) => {
            let network_config = get_network_config()?;
            skip_slots::run::<T>(env, network_config, matches)
                .map_err(|e| format!("Failed to skip slots: {}", e))
        }
        ("pretty-ssz", Some(matches)) => {
            let network_config = get_network_config()?;
            run_parse_ssz::<T>(network_config, matches)
                .map_err(|e| format!("Failed to pretty print hex: {}", e))
        }
        ("deploy-deposit-contract", Some(matches)) => {
            deploy_deposit_contract::run::<T>(env, matches)
                .map_err(|e| format!("Failed to run deploy-deposit-contract command: {}", e))
        }
        ("eth1-genesis", Some(matches)) => {
            let testnet_dir = get_testnet_dir()?;
            eth1_genesis::run::<T>(env, testnet_dir, matches)
                .map_err(|e| format!("Failed to run eth1-genesis command: {}", e))
        }
        ("interop-genesis", Some(matches)) => {
            let testnet_dir = get_testnet_dir()?;
            interop_genesis::run::<T>(testnet_dir, matches)
                .map_err(|e| format!("Failed to run interop-genesis command: {}", e))
        }
        ("change-genesis-time", Some(matches)) => {
            let testnet_dir = get_testnet_dir()?;
            change_genesis_time::run::<T>(testnet_dir, matches)
                .map_err(|e| format!("Failed to run change-genesis-time command: {}", e))
        }
        ("create-payload-header", Some(matches)) => create_payload_header::run::<T>(matches)
            .map_err(|e| format!("Failed to run create-payload-header command: {}", e)),
        ("replace-state-pubkeys", Some(matches)) => {
            let testnet_dir = get_testnet_dir()?;
            replace_state_pubkeys::run::<T>(testnet_dir, matches)
                .map_err(|e| format!("Failed to run replace-state-pubkeys command: {}", e))
        }
        ("new-testnet", Some(matches)) => {
            let testnet_dir = get_testnet_dir()?;
            new_testnet::run::<T>(testnet_dir, matches)
                .map_err(|e| format!("Failed to run new_testnet command: {}", e))
        }
        ("check-deposit-data", Some(matches)) => check_deposit_data::run(matches)
            .map_err(|e| format!("Failed to run check-deposit-data command: {}", e)),
        ("generate-bootnode-enr", Some(matches)) => generate_bootnode_enr::run::<T>(matches)
            .map_err(|e| format!("Failed to run generate-bootnode-enr command: {}", e)),
        ("insecure-validators", Some(matches)) => insecure_validators::run(matches)
            .map_err(|e| format!("Failed to run insecure-validators command: {}", e)),
        ("mnemonic-validators", Some(matches)) => mnemonic_validators::run(matches)
            .map_err(|e| format!("Failed to run mnemonic-validators command: {}", e)),
        ("indexed-attestations", Some(matches)) => indexed_attestations::run::<T>(matches)
            .map_err(|e| format!("Failed to run indexed-attestations command: {}", e)),
        ("block-root", Some(matches)) => {
            let network_config = get_network_config()?;
            block_root::run::<T>(env, network_config, matches)
                .map_err(|e| format!("Failed to run block-root command: {}", e))
        }
        ("state-root", Some(matches)) => {
            let network_config = get_network_config()?;
            state_root::run::<T>(env, network_config, matches)
                .map_err(|e| format!("Failed to run state-root command: {}", e))
        }
        ("mock-el", Some(matches)) => mock_el::run::<T>(env, matches)
            .map_err(|e| format!("Failed to run mock-el command: {}", e)),
        (other, _) => Err(format!("Unknown subcommand {}. See --help.", other)),
    }
}
