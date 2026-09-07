//! Native cache replies through the public encrypted session boundary.

use std::error::Error;

use solarity_network::{
    GameObjectQueryResponse, WorldAddonManifest, WorldAuthProgress, WorldConnection,
};

use super::{
    authenticate_worldserver, authenticated_identity_and_realm, runtime, write_encrypted_raw,
};

/// 0x0098D750 copies 24 properties, seven bounded byte strings, and six quest IDs.
#[test]
fn encrypted_game_object_query_preserves_native_template_and_packet_boundaries()
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
                write_encrypted_raw(&mut server, &mut crypto, 0x5f, &body).await?;
            }
            for body in malformed {
                write_encrypted_raw(&mut server, &mut crypto, 0x5f, &body).await?;
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
        assert_eq!(packet.name(), Some("SMSG_GAMEOBJECT_QUERY_RESPONSE"));
        let Some(GameObjectQueryResponse::Found(template)) = packet.game_object_query()? else {
            return Err("missing native template".into());
        };
        assert_eq!(
            (
                template.entry(),
                template.object_type(),
                template.display_id()
            ),
            (42, 15, 3015)
        );
        assert_eq!(template.strings()[0], [0xff, b'B']);
        assert_eq!(template.strings()[6], b"unknown");
        assert_eq!(
            template.properties(),
            &std::array::from_fn(|index| 0x9000_0000 + index as u32)
        );
        assert_eq!(template.scale(), 1.25);
        assert_eq!(template.quest_items(), &[101, 102, 103, 104, 105, 106]);
        let Some(GameObjectQueryResponse::Found(bounded)) =
            session.receive_packet().await?.game_object_query()?
        else {
            return Err("missing maximum-length template".into());
        };
        assert_eq!(bounded.strings()[0].len(), 1023);
        assert_eq!(
            session.receive_packet().await?.game_object_query()?,
            Some(GameObjectQueryResponse::Missing(42))
        );
        for _ in 0..malformed_count {
            assert!(session.receive_packet().await?.game_object_query().is_err());
        }
        let unrelated = session.receive_packet().await?;
        assert!(unrelated.game_object_query()?.is_none());
        assert_eq!(unrelated.payload(), &[7]);
        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// Layout recovered from the pinned executable rather than the six-word schema.
fn template_payload(name: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    for word in [42_u32, 15, 3015] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    for string in [
        name, b"second", b"", b"fourth", b"icon", b"caption", b"unknown",
    ] {
        body.extend_from_slice(string);
        body.push(0);
    }
    for word in 0x9000_0000_u32..0x9000_0018 {
        body.extend_from_slice(&word.to_le_bytes());
    }
    body.extend_from_slice(&1.25_f32.to_le_bytes());
    for item in 101_u32..107 {
        body.extend_from_slice(&item.to_le_bytes());
    }
    body
}
