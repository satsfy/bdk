use bdk_chain::bitcoin::{Address, Amount, ScriptPubKeyBuf};
use bdk_core::{
    bitcoin::{script::ScriptPubKeyBufExt as _, Network, TapNodeHash, XOnlyPublicKey},
    spk_client::SyncRequest,
    CheckPoint,
};
use bdk_electrum::BdkElectrumClient;
use bdk_testenv::{anyhow, TestEnv};
use criterion::{criterion_group, criterion_main, Criterion};
use electrum_client::ElectrumApi;
use std::{collections::BTreeSet, time::Duration};

// Batch size for `sync_with_electrum`.
const BATCH_SIZE: usize = 100;

pub fn get_test_spk(i: usize) -> ScriptPubKeyBuf {
    const PK_BYTES: &[u8] = &[
        12, 244, 72, 4, 163, 4, 211, 81, 159, 82, 153, 123, 125, 74, 142, 40, 55, 237, 191, 231,
        31, 114, 89, 165, 83, 141, 8, 203, 93, 240, 53, 101,
    ];
    let pk = XOnlyPublicKey::from_byte_array(PK_BYTES.try_into().expect("must be 32 bytes"))
        .expect("Must be valid PK");
    let mut merkle_root_bytes = [0u8; 32];
    merkle_root_bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
    ScriptPubKeyBuf::new_p2tr(pk, Some(TapNodeHash::from_byte_array(merkle_root_bytes)))
}

fn sync_with_electrum<E: ElectrumApi>(
    client: &BdkElectrumClient<E>,
    spks: &[ScriptPubKeyBuf],
    chain_tip: &CheckPoint,
) -> anyhow::Result<()> {
    let update = client.sync(
        SyncRequest::builder()
            .chain_tip(chain_tip.clone())
            .spks(spks.iter().cloned()),
        BATCH_SIZE,
        true,
    )?;

    assert!(
        !update.tx_update.txs.is_empty(),
        "expected some transactions from sync, but got none"
    );

    Ok(())
}

pub fn test_sync_performance(c: &mut Criterion) {
    let env = TestEnv::new().unwrap();

    const NUM_BLOCKS: usize = 100;
    let mut spks = Vec::with_capacity(NUM_BLOCKS);

    // Mine some blocks and send transactions.
    env.mine_blocks(101, None).unwrap();

    // Scatter UTXOs across many blocks.
    for i in 0..NUM_BLOCKS {
        let spk = get_test_spk(i);
        let addr = Address::from_script(&spk, Network::Regtest).unwrap();
        env.send(&addr, Amount::from_sat_u32(10_000)).unwrap();
        env.mine_blocks(1, None).unwrap();

        spks.push(spk);
    }
    let _ = env.wait_until_electrum_sees_block(Duration::from_secs(6));
    assert_eq!(
        spks.iter().cloned().collect::<BTreeSet<_>>().len(),
        spks.len(),
        "all spks must be unique",
    );

    // Setup receiver.
    let genesis_cp = CheckPoint::new(0, env.genesis_hash().unwrap());

    {
        let electrum_client =
            electrum_client::Client::new(env.electrsd.electrum_url.as_str()).unwrap();
        let spks = spks.clone();
        let genesis_cp = genesis_cp.clone();
        c.bench_function("sync_with_electrum", move |b| {
            b.iter(|| {
                sync_with_electrum(
                    &BdkElectrumClient::new(&electrum_client),
                    &spks,
                    &genesis_cp,
                )
                .expect("must not error")
            })
        });
    }

    {
        let client = BdkElectrumClient::new(
            electrum_client::Client::new(env.electrsd.electrum_url.as_str()).unwrap(),
        );
        c.bench_function("sync_with_electrum_cached", move |b| {
            b.iter(|| sync_with_electrum(&client, &spks, &genesis_cp).expect("must not error"))
        });
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10);
    targets = test_sync_performance
}
criterion_main!(benches);
