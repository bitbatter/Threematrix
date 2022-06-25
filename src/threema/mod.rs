use threema_gateway::{ApiBuilder, E2eApi, IncomingMessage};

use crate::threema::serialization::encrypt_group_sync_req_msg;
use crate::threema::types::{GroupCreateMessage, GroupRenameMessage, GroupTextMessage, MessageBase, MessageType, TextMessage};

use self::serialization::encrypt_group_text_msg;
use self::types::Message;

pub mod serialization;
pub mod types;

pub struct ThreemaClient {
    api: E2eApi,
}

const GROUP_ID_NUM_BYTES: usize = 8;
const GROUP_CREATOR_NUM_BYTES: usize = 8;
const MESSAGE_TYPE_NUM_BYTES: usize = 1;
const THREEMA_ID_LENGTH: usize = 8;

impl ThreemaClient {
    pub fn new(own_id: &str, secret: &str, private_key: &str) -> ThreemaClient {
        let api = ApiBuilder::new(own_id, secret)
            .with_private_key_str(private_key.as_ref())
            .and_then(|builder| builder.into_e2e())
            .unwrap();
        return ThreemaClient { api: api };
    }

    pub async fn send_group_msg(
        &self,
        text: &str,
        group_creator: &str,
        group_id: &[u8],
        receivers: &[&str],
    ) -> () {
        for user_id in receivers {
            let public_key = self.api.lookup_pubkey(*user_id).await.unwrap();
            let encrypted_msg = encrypt_group_text_msg(
                text,
                group_creator,
                group_id,
                &public_key.into(),
                &self.api,
            );

            match &self.api.send(&user_id, &encrypted_msg, false).await {
                Ok(msg_id) => println!("Sent. Message id is {}.", msg_id),
                Err(e) => println!("Could not send message: {:?}", e),
            }
        }
    }

    pub async fn send_group_sync_req_msg(
        &self,
        group_id: &[u8],
        receiver: &str,
    ) -> () {
        let public_key = self.api.lookup_pubkey(receiver).await.unwrap();
        let encrypted_message = encrypt_group_sync_req_msg(group_id, &public_key.into(), &self.api);
        match &self.api.send(receiver, &encrypted_message, false).await {
            Ok(msg_id) => println!("Sent. Message id is {}.", msg_id),
            Err(e) => println!("Could not send message: {:?}", e),
        }
    }


    pub async fn process_incoming_msg(
        &self,
        incoming_message: &IncomingMessage,
    ) -> Result<Message, ()> {
        println!("Parsed and validated message from request:");
        println!("  From: {}", incoming_message.from);
        println!("  Sender nickname: {:?}", incoming_message.nickname);
        println!("  To: {}", incoming_message.to);
        println!("  Message ID: {}", incoming_message.message_id);
        println!("  Timestamp: {}", incoming_message.date);

        // Fetch sender public key
        let pubkey = self
            .api
            .lookup_pubkey(&incoming_message.from)
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "Could not fetch public key for {}: {:?}",
                    &incoming_message.from, e
                );
                std::process::exit(1);
            });

        // Decrypt
        let data = self
            .api
            .decrypt_incoming_message(&incoming_message, &pubkey)
            .unwrap_or_else(|e| {
                println!("Could not decrypt box: {:?}", e);
                std::process::exit(1);
            });

        let message_type: u8 = &data[0] & 0xFF;
        println!("  MessageType: {:#02x}", message_type);


        let base = MessageBase {
            from_identity: incoming_message.from.clone(),
            to_identity: incoming_message.to.clone(),
            push_from_name: incoming_message.nickname.clone(),
            message_id: incoming_message.message_id.clone(),
            date: incoming_message.date as u64,
        };

        match MessageType::from(message_type) {
            MessageType::Text => {
                let text = String::from_utf8(data[MESSAGE_TYPE_NUM_BYTES..].to_vec()).unwrap();
                println!("  text: {}", text);
                return Ok(Message::TextMessage(TextMessage {
                    base,
                    text,
                }));
            }
            MessageType::GroupText => {
                let group_creator = String::from_utf8(data[MESSAGE_TYPE_NUM_BYTES..MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES].to_vec()).unwrap();
                let group_id = &data[MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES..MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES + GROUP_ID_NUM_BYTES];
                let text = String::from_utf8(data[MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES + GROUP_ID_NUM_BYTES..].to_vec()).unwrap();

                // Show result
                println!("  GroupCreator: {}", group_creator);
                println!("  groupId: {:?}", group_id);
                println!("  text: {}", text);

                return Ok(Message::GroupTextMessage(GroupTextMessage {
                    base,
                    text,
                    group_creator,
                    group_id: group_id.to_vec(),
                }));
            }
            MessageType::GroupCreate => {
                let group_id = &data[MESSAGE_TYPE_NUM_BYTES..MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES];
                let mut members: Vec<String> = Vec::new();

                let mut counter = 0;
                let mut current_member_id = "".to_owned();
                for char in &data[MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES..] {
                    current_member_id = current_member_id + String::from_utf8(vec!(*char)).unwrap().as_str();
                    counter = counter + 1;
                    if counter == THREEMA_ID_LENGTH {
                        members.push(current_member_id.clone());
                        current_member_id = "".to_owned();
                        counter = 0;
                    }
                }

                return Ok(Message::GroupCreateMessage(GroupCreateMessage {
                    base,
                    members,
                    group_id: group_id.to_vec(),
                }));
            }
            MessageType::GroupRename => {
                let group_id = &data[MESSAGE_TYPE_NUM_BYTES..GROUP_ID_NUM_BYTES];
                let group_name = String::from_utf8(data[MESSAGE_TYPE_NUM_BYTES + GROUP_CREATOR_NUM_BYTES..].to_vec()).unwrap();

                return Ok(Message::GroupRenameMessage(GroupRenameMessage {
                    base,
                    group_name,
                    group_id: group_id.to_vec(),
                }));
            }
            // MessageType::GroupRequestSync => {}
            // MessageType::Image => {}
            // MessageType::Video => {}
            // MessageType::File => {}
            // MessageType::DeliveryReceipt => {}
            _ => {
                println!("Unknown message type received");
                println!("  content: {:?}", &data[1..]);
                Err(())
            }
        }
    }
}
