use futures::StreamExt;
use serenity::{
    all::{
        Color, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateForumPost,
        CreateMessage, EditMessage, EditThread,
    },
    async_trait,
    http::CacheHttp,
    model::{channel::Channel, gateway::Ready},
    prelude::*,
};
use std::collections::BTreeMap;
use subxt::{
    ext::codec::{Decode, Encode},
    utils::AccountId32,
    OnlineClient, PolkadotConfig,
};

const ENDPOINT: &str = "wss://invarch-tinkernet.api.onfinality.io:443/public-ws";

#[subxt::subxt(runtime_metadata_url = "wss://invarch-tinkernet.api.onfinality.io:443/public-ws")]
//#[subxt::subxt(runtime_metadata_url = "ws://localhost:9944")]
pub mod tinkernet {}

use tinkernet::runtime_types::{
    pallet_inv4::{
        pallet::{Call as INV4Call, Event as INV4Event},
        voting::Vote,
    },
    tinkernet_runtime::{RuntimeCall, RuntimeEvent},
};

const CORE_ID: u32 = 0;
const TOKEN_DECIMALS: u128 = 1000000;
const CHANNEL_ID: u64 = 1089713385475674172;

#[derive(Encode, Decode)]
struct EmbedData {
    message_id: u64,
    author: AccountId32,
    voters: BTreeMap<AccountId32, Vote<u128>>,
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let http = ctx.http();

        let api = OnlineClient::<PolkadotConfig>::from_url(ENDPOINT)
            .await
            .unwrap();
        let mut block_sub = api.blocks().subscribe_finalized().await.unwrap();

        while let Some(block) = block_sub.next().await {
            let block = block.unwrap();

            let events = block.events().await.unwrap();

            for event in events.iter() {
                let event = event.unwrap();

                if let Ok(RuntimeEvent::INV4(inv4_event)) =
                    event.as_root_event::<tinkernet::Event>()
                {
                    if let Ok(Channel::Guild(channel)) =
                        ctx.http.get_channel(CHANNEL_ID.into()).await
                    {
                        match inv4_event {
                            INV4Event::MultisigVoteStarted {
                                core_id,
                                executor_account,
                                voter,
                                votes_added,
                                call_hash,
                                call,
                            } if core_id == CORE_ID => {
                                let id = channel.create_forum_post(http, CreateForumPost::new(hex::encode(call_hash), CreateMessage::new().embed( CreateEmbed::new()
                                                .title("New Multisig Call")
                                                .description(format!("Core ID: {core_id}, account: {executor_account}"))
                                                .author(CreateEmbedAuthor::new(format!("Author: {}", voter)))
                                                .color(Color::PURPLE)
                                                .field(
                                                    "Aye Votes",
                                                    match votes_added {
                                                        Vote::Aye(v) => v / TOKEN_DECIMALS,
                                                        _ => 0u128,
                                                    }
                                                    .to_string(),
                                                    false,
                                                )
                                                .field("Nay Votes", "0", false)
                                                .field("Voters", format!("{voter} - {} - Aye", match votes_added {
                                                    Vote::Aye(v) => v / TOKEN_DECIMALS,
                                                    _ => 0u128,
                                                }), false)
                                                .field("Call Hash", format!("0x{}", hex::encode(call_hash)), false)
                                                .field("Call", format!("{:?}", call.try_decode().unwrap()), false)
                                ).components(vec![CreateActionRow::Buttons(vec![CreateButton::new_link(format!("https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Finvarch-tinkernet.api.onfinality.io%2Fpublic-ws#/extrinsics/decode/0x{}", hex::encode(RuntimeCall::INV4(INV4Call::vote_multisig { core_id: CORE_ID, call_hash, aye: true }).encode()))).label("Vote")])]
                                        )))
                                    .await
                                    .unwrap().id;

                                let data = ctx.data.write().await;
                                if let Some(db) = data.get::<DbData>() {
                                    let _ = db.insert(
                                        call_hash.encode(),
                                        EmbedData {
                                            message_id: id.0.into(),
                                            author: voter.clone(),
                                            voters: BTreeMap::from([(voter, votes_added)]),
                                        }
                                        .encode(),
                                    );
                                }
                            }

                            INV4Event::MultisigVoteAdded {
                                core_id,
                                executor_account,
                                voter,
                                votes_added,
                                current_votes,
                                call_hash,
                                call,
                            } if core_id == CORE_ID => {
                                let data = ctx.data.write().await;
                                if let Some(db) = data.get::<DbData>() {
                                    if let Ok(Some(EmbedData {
                                        message_id,
                                        author,
                                        voters,
                                    })) = db.get(call_hash.encode()).map(|o| {
                                        o.map(|v| {
                                            EmbedData::decode(&mut v.to_vec().as_slice()).unwrap()
                                        })
                                    }) {
                                        let mut new_voters = voters;
                                        new_voters.insert(voter, votes_added);

                                        let mut list = String::new();
                                        new_voters.iter().for_each(|(vt, vote)| {
                                            let (l, r) = match vote {
                                                Vote::Aye(v) => (v / TOKEN_DECIMALS, "Aye"),
                                                Vote::Nay(v) => (v / TOKEN_DECIMALS, "Nay"),
                                            };

                                            list.push_str(
                                                format!("\n{vt} - {} - {}", l, r).as_str(),
                                            )
                                        });

                                        if let Ok(Channel::Guild(post)) =
                                            ctx.http.get_channel(message_id.into()).await
                                        {
                                            let _ = post
                                            .edit_message(http, message_id, EditMessage::new()
                                                .embed( CreateEmbed::new()
                                                .title("New Multisig Call")
                                                .description(format!("Core ID: {core_id}, account: {executor_account}"))
                                                .author( CreateEmbedAuthor::new(format!("Author: {}", author)))
                                                .color(Color::PURPLE)
                                                .field(
                                                    "Aye Votes",
                                                    (current_votes.ayes / TOKEN_DECIMALS).to_string(),
                                                    false,
                                                )
                                                .field("Nay Votes", (current_votes.nays / TOKEN_DECIMALS).to_string(), false)
                                                .field("Voters", list, false)
                                                .field("Call Hash", format!("0x{}", hex::encode(call_hash)), false)
                                                .field("Call", format!("{:?}", call.try_decode().unwrap()), false)
                                        )
                                            ).await;
                                        }

                                        let _ = db.insert(
                                            call_hash.encode(),
                                            EmbedData {
                                                message_id,
                                                voters: new_voters,
                                                author,
                                            }
                                            .encode(),
                                        );
                                    }
                                }
                            }

                            INV4Event::MultisigExecuted {
                                core_id,
                                executor_account,
                                voter,
                                call_hash,
                                call,
                                result,
                            } if core_id == CORE_ID => {
                                let data = ctx.data.write().await;
                                if let Some(db) = data.get::<DbData>() {
                                    if let Ok(Some(EmbedData { message_id, .. })) =
                                        db.get(call_hash.encode()).map(|o| {
                                            o.map(|v| {
                                                EmbedData::decode(&mut v.to_vec().as_slice())
                                                    .unwrap()
                                            })
                                        })
                                    {
                                        if let Ok(Channel::Guild(mut post)) =
                                            ctx.http.get_channel(message_id.into()).await
                                        {
                                            let _ =  post
                                    .send_message(http, CreateMessage::new()
                                       .embed(CreateEmbed::new()
                                                .title("Multisig Call Executed")
                                                .description(format!("Core ID: {core_id}, account: {executor_account}"))
                                                .author(CreateEmbedAuthor::new(format!("Last voter: {}", voter)))
                                                .color(if result.is_ok() {Color::DARK_GREEN} else {Color::RED})
                                                .field("Call Hash", format!("0x{}", hex::encode(call_hash)), false)
                                                .field("Call", format!("{:?}", call.try_decode().unwrap()), false)
                                                .field("Result", if result.is_err() {"Error"} else {"Successful"}, false)
                                        )
                                    )
                                    .await;

                                            let _ = post
                                                .edit_thread(
                                                    http,
                                                    EditThread::new().archived(true).locked(true),
                                                )
                                                .await;

                                            if let Some(db) = data.get::<DbData>() {
                                                let _ = db.remove(call_hash.encode());
                                            }
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }
}

struct DbData;

impl TypeMapKey for DbData {
    type Value = sled::Db;
}

#[tokio::main]
async fn main() {
    let token = dotenv::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    client
        .data
        .write()
        .await
        .insert::<DbData>(sled::open("multisig_db").unwrap());

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
