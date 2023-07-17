use super::{IbcV0, IbcV1};
use crate::{describe::KeyOp, migrate::Migrate, state::State, store::Store};

impl Migrate for IbcV1 {
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        if bytes[0] == 1 {
            *bytes = &bytes[1..];
            return Ok(Self {
                height: Migrate::migrate(
                    <Self as State>::field_keyop("height")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("height")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                host_consensus_states: Migrate::migrate(
                    <Self as State>::field_keyop("host_consensus_states")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("host_consensus_states")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                channel_counter: Migrate::migrate(
                    <Self as State>::field_keyop("channel_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("channel_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                connection_counter: Migrate::migrate(
                    <Self as State>::field_keyop("connection_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("connection_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                client_counter: Migrate::migrate(
                    <Self as State>::field_keyop("client_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("client_counter")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                transfer: Migrate::migrate(
                    <Self as State>::field_keyop("transfer")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("transfer")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                clients: Migrate::migrate(
                    <Self as State>::field_keyop("clients")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("clients")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                connections: Migrate::migrate(
                    <Self as State>::field_keyop("connections")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("connections")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                channel_ends: Migrate::migrate(
                    <Self as State>::field_keyop("channel_ends")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("channel_ends")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                next_sequence_send: Migrate::migrate(
                    <Self as State>::field_keyop("next_sequence_send")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("next_sequence_send")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                next_sequence_recv: Migrate::migrate(
                    <Self as State>::field_keyop("next_sequence_recv")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("next_sequence_recv")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                next_sequence_ack: Migrate::migrate(
                    <Self as State>::field_keyop("next_sequence_ack")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("next_sequence_ack")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                commitments: Migrate::migrate(
                    <Self as State>::field_keyop("commitments")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("commitments")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                receipts: Migrate::migrate(
                    <Self as State>::field_keyop("receipts")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("receipts")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                acks: Migrate::migrate(
                    <Self as State>::field_keyop("acks")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("acks")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
                store: Migrate::migrate(
                    <Self as State>::field_keyop("store")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&src),
                    <Self as State>::field_keyop("store")
                        .unwrap_or(KeyOp::Append(vec![]))
                        .apply(&dest),
                    bytes,
                )?,
            });
        }

        let mut other = IbcV0::migrate(src, dest, bytes)?;

        other
            .root_store
            .remove_range(b"a".to_vec()..b"z".to_vec())?;

        other.local_store.remove_range(..)?;

        let mut out = vec![];
        other.root_store.flush(&mut out)?;
        other.local_store.flush(&mut out)?;

        Ok(Self::default())
    }
}
