use alloy_primitives::{address, hex, utils::Unit, Address, Bytes, FixedBytes, U160, U256};
use axum::http::{HeaderName, HeaderValue};
use foundry_common::{selectors::OpenChainClient, ALCHEMY_FREE_TIER_CUPS, REQUEST_TIMEOUT};
use hyper::HeaderMap;
use revm::{precompile::Error, primitives::{Precompile, PrecompileResult, PrecompileError}};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use std::{fmt::Debug, sync::Arc};

use crate::{config::{DEFAULT_MNEMONIC, NODE_PORT}, AccountGenerator, NodeConfig, PrecompileFactory};
use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

const REQ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize)]
struct AnvilRequest {
    pub method: String,
    pub params: Vec<String>,
    pub id: u64,
    pub jsonrpc: String
}

#[derive(Serialize, Deserialize)]
pub struct BalanceResponse {
    #[serde(rename = "jsonrpc")]
    jsonrpc: String,

    #[serde(rename = "id")]
    id: i64,

    #[serde(rename = "result")]
    result: String,
}


impl NodeConfig {
    pub fn celo() -> Self {

        const PRECOMPILE_ADDR: Address = address!("0000000000000000000000000000000000000071");
        fn my_precompile(_bytes: &Bytes, _gas_limit: u64) -> PrecompileResult {
          
            println!("received bytes {}", _bytes);
            println!("Gas limit {}", _gas_limit);

            if (_bytes.len() < 100) {
                return Err(PrecompileError::other("from, to, value is required"));
            }

            let mut fixed_bytes: [u8; 32] = [0; 32];

            fixed_bytes.copy_from_slice(&_bytes[4..36]);
            let from = Address::from_word(FixedBytes::new(fixed_bytes));

            fixed_bytes.copy_from_slice(&_bytes[36..68]);
            let to = Address::from_word(FixedBytes::new(fixed_bytes));

            fixed_bytes.copy_from_slice(&_bytes[68..100]);
            let value = U256::from_be_bytes(fixed_bytes);

            let client = reqwest::Client::builder()
            .default_headers( {
                let mut headers = HeaderMap::new();
                headers.insert(HeaderName::from_static("user-agent"), HeaderValue::from_static("forge"));
                headers.insert(HeaderName::from_static("content-type"), HeaderValue::from_static("application/json"));
                headers
            }
            )
            .timeout(REQ_TIMEOUT)
            .build().unwrap();

            let request_get_balance_from = AnvilRequest {
                method: "eth_getBalance".to_string(),
                id: 1,
                jsonrpc: "2.0".to_string(),
                params: vec![from.to_string(), "latest".to_string()]
            };

            let request_get_balance_to = AnvilRequest {
                method: "eth_getBalance".to_string(),
                id: 1,
                jsonrpc: "2.0".to_string(),
                params: vec![to.to_string(), "latest".to_string()]
            };

            let mut request_set_balance_from = AnvilRequest {
                method: "anvil_setBalance".to_string(),
                id: 1,
                jsonrpc: "2.0".to_string(),
                params: vec![from.to_string(), value.to_string()]
            };

            let mut request_set_balance_to = AnvilRequest {
                method: "anvil_setBalance".to_string(),
                id: 1,
                jsonrpc: "2.0".to_string(),
                params: vec![to.to_string(), value.to_string()]
            };

            tokio::spawn(async move {

                let balance_from_future = client
                    .post("http://localhost:8545")
                    .json(&request_get_balance_from)
                    .send();

                let balance_to_future = client
                    .post("http://localhost:8545")
                    .json(&request_get_balance_to)
                    .send();
                    
                let balance_from = balance_from_future.await.expect("Didn't receive balance from").json::<BalanceResponse>().await.expect("Parsing of balance from failed");
                let balance_to = balance_to_future.await.expect("Didn't receive balance to").json::<BalanceResponse>().await.expect("Parsing of balance to failed");

                println!("balance_from {}", balance_from.result);
                println!("balance_to {}", balance_to.result);

                let balance_from_uint = U256::from_str_radix(&balance_from.result.split_at(2).1, 16).expect("problem parsing from uint");
                let balance_to_uint = U256::from_str_radix(&balance_to.result.split_at(2).1, 16).expect("problem parsing to uint");


                if (balance_from_uint < value) {
                    return;
                }

                request_set_balance_from.params[1] = (balance_from_uint - value).to_string();
                request_set_balance_to.params[1] = (balance_to_uint + value).to_string();

                println!("balance from {}", request_set_balance_from.params[0]);
                println!("balance to {}", request_set_balance_to.params[0]);

                println!("in block");
                client
                .post("http://localhost:8545")
                .json(&request_set_balance_from)
                .send()
                .await
                .unwrap();

                client
                .post("http://localhost:8545")
                .json(&request_set_balance_to)
                .send()
                .await
                .unwrap();
                println!("after call");
            });

            Ok((0, Bytes::from_static(&hex!("7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b8082525050506014600cf3"))))
        }

        #[derive(Debug)]
        struct CustomPrecompileFactory;

        impl PrecompileFactory for CustomPrecompileFactory {
            fn precompiles(&self) -> Vec<(Address, Precompile)> {
                vec![(PRECOMPILE_ADDR, Precompile::Standard(my_precompile))]
            }
        }

        // generate some random wallets
        let genesis_accounts = AccountGenerator::new(10).phrase(DEFAULT_MNEMONIC).gen();
        Self {
            chain_id: None,
            gas_limit: 30_000_000,
            disable_block_gas_limit: false,
            gas_price: None,
            hardfork: None,
            signer_accounts: genesis_accounts.clone(),
            genesis_timestamp: None,
            genesis_accounts,
            // 100ETH default balance
            genesis_balance: Unit::ETHER.wei().saturating_mul(U256::from(100u64)),
            block_time: None,
            no_mining: false,
            port: NODE_PORT,
            // TODO make this something dependent on block capacity
            max_transactions: 1_000,
            silent: false,
            eth_rpc_url: None,
            fork_block_number: None,
            account_generator: None,
            base_fee: None,
            blob_excess_gas_and_price: None,
            enable_tracing: true,
            enable_steps_tracing: false,
            enable_auto_impersonate: false,
            no_storage_caching: false,
            server_config: Default::default(),
            host: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            transaction_order: Default::default(),
            config_out: None,
            genesis: None,
            fork_request_timeout: REQUEST_TIMEOUT,
            fork_headers: vec![],
            fork_request_retries: 5,
            fork_retry_backoff: Duration::from_millis(1_000),
            fork_chain_id: None,
            // alchemy max cpus <https://docs.alchemy.com/reference/compute-units#what-are-cups-compute-units-per-second>
            compute_units_per_second: ALCHEMY_FREE_TIER_CUPS,
            ipc_path: None,
            code_size_limit: None,
            prune_history: Default::default(),
            init_state: None,
            transaction_block_keeper: None,
            disable_default_create2_deployer: false,
            enable_optimism: false,
            slots_in_an_epoch: 32,
            memory_limit: None,
            precompile_factory: Some(Arc::new(CustomPrecompileFactory))
        }
    }
}
