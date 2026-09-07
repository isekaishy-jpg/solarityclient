//! Native cache replies through the public encrypted session boundary.

use std::error::Error;

use solarity_network::{
    CreatureQueryResponse, WorldAddonManifest, WorldAuthProgress, WorldConnection,
};

use super::{
    authenticate_worldserver, authenticated_identity_and_realm, runtime, write_encrypted_raw,
};

/// 98D4C0 retains six byte strings, full-width words, two floats and a normalized byte.
#[test]
fn encrypted_creature_query_preserves_native_template_and_packet_boundaries()
-> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, mut server) = tokio::io::duplex(4096);
        let valid = template_payload(&[0xff, b'B']);
        let mut malformed = (0..valid.len())
            .map(|length| valid[..length].to_vec())
            .collect::<Vec<_>>();
        let mut trailing = valid.clone();
        trailing.push(0);
        malformed.push(trailing);
        malformed.push(template_payload(&[b'x'; 1024]));
        let mut missing_trailing = 0x8000_002a_u32.to_le_bytes().to_vec();
        missing_trailing.push(0);
        malformed.push(missing_trailing);
        let malformed_count = malformed.len();
        let server_task = tokio::spawn(async move {
            let mut crypto = authenticate_worldserver(&mut server, session_key).await?;
            for body in [
                valid,
                template_payload(&[b'x'; 1023]),
                0x8000_002a_u32.to_le_bytes().to_vec(),
            ] {
                write_encrypted_raw(&mut server, &mut crypto, 0x61, &body).await?;
            }
            for body in malformed {
                write_encrypted_raw(&mut server, &mut crypto, 0x61, &body).await?;
            }
            write_encrypted_raw(&mut server, &mut crypto, 0x123, &[7]).await?;
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        });
        let mut session = match WorldConnection::authenticate(
            client,
            identity,
            &realm,
            WorldAddonManifest::empty(),
        )
        .await?
        {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => return Err("fixture unexpectedly queued".into()),
        };
        let packet = session.receive_packet().await?;
        assert_eq!(packet.name(), Some("SMSG_CREATURE_QUERY_RESPONSE"));
        let Some(CreatureQueryResponse::Found(template)) = packet.creature_query()? else {
            return Err("missing native template".into());
        };
        assert_eq!(template.entry(), 42);
        assert_eq!(template.strings()[0], [0xff, b'B']);
        assert_eq!(template.strings()[5], b"description");
        assert_eq!(template.flags(), 0x0240_0000);
        assert_eq!(template.creature_type(), 0xffff_ffff);
        assert_eq!(template.family(), 0xffff_fffe);
        assert_eq!(template.rank(), 0xffff_fffd);
        assert_eq!(template.kill_credits(), &[71, 72]);
        assert_eq!(template.display_ids(), &[81, 82, 83, 84]);
        assert_eq!(template.health_multiplier(), 1.25);
        assert_eq!(template.mana_multiplier(), 0.5);
        assert!(template.racial_leader());
        assert_eq!(template.quest_items(), &[101, 102, 103, 104, 105, 106]);
        assert_eq!(template.movement_id(), 0xabcdef12);
        let Some(CreatureQueryResponse::Found(bounded)) =
            session.receive_packet().await?.creature_query()?
        else {
            return Err("missing maximum-length template".into());
        };
        assert_eq!(bounded.strings()[0].len(), 1023);
        assert_eq!(
            session.receive_packet().await?.creature_query()?,
            Some(CreatureQueryResponse::Missing(42))
        );
        for _ in 0..malformed_count {
            assert!(session.receive_packet().await?.creature_query().is_err());
        }
        let unrelated = session.receive_packet().await?;
        assert!(unrelated.creature_query()?.is_none());
        assert_eq!(unrelated.payload(), &[7]);
        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// Complete scalar and bounded-string order recovered from native 98D4C0.
fn template_payload(name: &[u8]) -> Vec<u8> {
    let mut body = 42_u32.to_le_bytes().to_vec();
    for string in [name, b"second", b"", b"fourth", b"sub-name", b"description"] {
        body.extend_from_slice(string);
        body.push(0);
    }
    for word in [
        0x0240_0000_u32,
        0xffff_ffff,
        0xffff_fffe,
        0xffff_fffd,
        71,
        72,
        81,
        82,
        83,
        84,
    ] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    body.extend_from_slice(&1.25_f32.to_le_bytes());
    body.extend_from_slice(&0.5_f32.to_le_bytes());
    body.push(0xfe);
    for item in 101_u32..107 {
        body.extend_from_slice(&item.to_le_bytes());
    }
    body.extend_from_slice(&0xabcdef12_u32.to_le_bytes());
    body
}
