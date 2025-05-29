use prost::Message;
use proto::{Instruction, InstructionBatch};
use rand::{thread_rng, Rng, RngCore};
use sha2::{Digest, Sha256};
use tokio::{time, net::{TcpListener, TcpStream}, sync::mpsc};
use tokio_tungstenite::{accept_hdr_async, connect_async, tungstenite::{self, http::{Request, Response}}, MaybeTlsStream, WebSocketStream};
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use ed25519_dalek::{SigningKey, Signature, VerifyingKey, Verifier, ed25519::signature::SignerMut};
use std::{collections::{HashMap, HashSet}, env, fs::File, hash::{Hash, Hasher}, io::{self, BufRead}, process, sync::Arc, thread::{self}, time::Duration};

mod proto {
    include!("ota_integration.rs");
}

enum WebSocketReader {
    NonTls(SplitStream<WebSocketStream<TcpStream>>),
    MaybeTls(SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>)
}

impl WebSocketReader {
    async fn next(& mut self) -> Option<Result<tungstenite::Message, tungstenite::Error>> {
        match self {
            WebSocketReader::NonTls(read) => read.next().await,
            WebSocketReader::MaybeTls(read) => read.next().await
        }
    }
}

enum WebSocketWriter {
    NonTls(SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>),
    MaybeTls(SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>)
}

impl WebSocketWriter {
    async fn send(& mut self, message: tokio_tungstenite::tungstenite::Message) -> Result<(), tokio_tungstenite::tungstenite::Error>{
        match self {
            WebSocketWriter::NonTls(write) => write.send(message).await,
            WebSocketWriter::MaybeTls(write) => write.send(message).await
        }
    }
}

struct MessageToUnionizer {
    instruction_batch: InstructionBatch,
    peer_id: i32
}

// TO DO: Use this!! 
// impl PartialEq for Instruction {
//     fn eq(&self, other: &Self) -> bool {
//         self.property_id == other.property_id && self.message_id == other.message_id
//     }
// }

impl Eq for Instruction {}

impl Hash for Instruction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.property_id.hash(state);
        self.message_id.hash(state);
    }
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    let (id, address) = parse_cmd_line_args();
    println!("My id is {}, my address is: {}", id, address);

    let peers = get_peers(id);

    let (
        transmitter_to_unionizer, 
        unionizer_receiver) = mpsc::unbounded_channel::<MessageToUnionizer>();
    
    let unionizer_to_peer_channel_map = get_unionizer_to_peer_channel_map(&peers);
    let split_receivers = split_channel_map_to_lower_and_higher(id, unionizer_to_peer_channel_map.1);
    wait_for_lower_peers_to_init(id);
    let accept_connections_handle: tokio::task::JoinHandle<()> = accept_connections_from_higher_peers(&address, &transmitter_to_unionizer, split_receivers.1);
    connect_to_lower_peers(id, peers, &transmitter_to_unionizer, split_receivers.0); // TO DO wait these connections to finish as well... for now it is ok like this
    start_unionizer(id, unionizer_receiver, unionizer_to_peer_channel_map.0);
    
    accept_connections_handle.await.unwrap();

    Ok(())
}

fn get_unionizer_to_peer_channel_map(peers: &Vec<i32>) -> 
    (HashMap<i32, mpsc::UnboundedSender<Arc<HashSet<Instruction>>>>, HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>) {
    let mut res = (HashMap::new(), HashMap::new());
    for peer in peers {
        let (transmitter, receiver) = mpsc::unbounded_channel::<Arc<HashSet<Instruction>>>();
        res.0.insert(*peer, transmitter);
        res.1.insert(*peer, receiver);
    }
    res
}

fn split_channel_map_to_lower_and_higher(my_id: i32, original: HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>) 
-> (HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>, HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>) {
    let mut res = (HashMap::new(), HashMap::new());
    for elem in original {
        if elem.0 < my_id {
            res.0.insert(elem.0, elem.1);
        } else {
            res.1.insert(elem.0, elem.1);
        }
    }
    res
}

fn wait_for_lower_peers_to_init(id: i32) {
    thread::sleep(Duration::new(0, 100_000_000 * id as u32));
}

fn parse_cmd_line_args() -> (i32, String) {
    let args: Vec<String> = env::args().collect();

    let addr = if args.len() > 1 {
        args[1].clone()
    } else {
        println!("Error! You have to provide the full address for the connector.");
        process::exit(1);
    };

    let id: i32 = if args.len() > 2 {
        args[2].parse().unwrap()
    } else {
        println!("Error! You have to provide connector id.");
        process::exit(1);
    };

    (id, addr)
}

fn get_peers(id: i32) -> Vec<i32> {
    let mut neighbors:Vec<i32> = vec![];
    let path = "graph.txt";
    let file = File::open(path).expect("Error");
    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        let line = line.expect("Error");
        let numbers: Result<Vec<i32>, _> = line
            .split_whitespace()
            .map(str::parse)
            .collect();

        match numbers {
            Ok(nums) => {
                if nums[0] == id {
                    neighbors = (&nums[1..]).to_vec();
                    println!("My line is {:?}", neighbors);
                }
            }
            Err(e) => {
                eprintln!("Error parsing numbers: {}", e);
                process::exit(1);
            }
        }
    }

    neighbors
}

fn connect_to_lower_peers(id: i32, peers: Vec<i32>, transmitter_to_unionizer: &mpsc::UnboundedSender<MessageToUnionizer>,
    mut receivers_from_unionizer: HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>) {
    for peer in peers {
        if id > peer {
            let neighbor_port = 8080 + peer;
            let neighbor_addr = format!("ws://127.0.0.1:{}/socket/{}", neighbor_port, id);
            println!("Connecting to server at: {} peer id: {}", neighbor_addr, peer);
            let receiver_from_unionizer = receivers_from_unionizer.remove(&peer).unwrap();
            tokio::spawn(connect_to_server(peer, neighbor_addr.clone(), transmitter_to_unionizer.clone(), receiver_from_unionizer));
        }
    }
}

fn accept_connections_from_higher_peers(address: &String, transmitter_to_unionizer: &mpsc::UnboundedSender<MessageToUnionizer>, 
        mut receivers_from_unionizer: HashMap<i32, mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>>) -> tokio::task::JoinHandle<()> {
    let address = address.clone();
    let transmitter_to_unionizer = transmitter_to_unionizer.clone();
    return tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        println!("Listening for connections at: {}", address);

        while let Ok((stream, _)) = listener.accept().await {
            let address = &stream.peer_addr().unwrap().to_string();
            let mut peer_id = -1;

            let callback = |req: &Request<()>, response: Response<()>| {
                if let Some(last_index) = req.uri().path().rfind('/') {
                    peer_id = req.uri().path().get(last_index+1..).unwrap().parse().unwrap();
                } else {
                    println!("Error. Request uri was not in the right format.");
                }

                Ok(response)
            };

            let ws_stream = accept_hdr_async(stream, callback)
            .await
            .expect("Error during the websocket handshake occurred");

            if peer_id >= 0 {
                println!("Accepted client from: {}, peer id: {}.", address, peer_id);
                let receiver_from_unionizer = receivers_from_unionizer.remove(&peer_id).unwrap();
                tokio::spawn(accept_peer(ws_stream, transmitter_to_unionizer.clone(), address.clone(), receiver_from_unionizer));
            }
        }
    });
}

fn start_unionizer(id: i32, mut unionizer_receiver: mpsc::UnboundedReceiver<MessageToUnionizer>, transmitters_from_unionizer: HashMap<i32, mpsc::UnboundedSender<Arc<HashSet<Instruction>>>>) {
    let mut interval = time::interval(Duration::from_secs(1));
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            let mut union: HashSet<Instruction> = HashSet::new();
            loop {
                let message = unionizer_receiver.try_recv();

                match message {
                    Ok(message) => {
                        for instruction in &message.instruction_batch.instructions {
                            union.insert(instruction.clone());
                        }
                        // received_batches.insert(1, message.instruction_batch);
                    },
                    Err(_) => {
                        break;
                    }
                }
            }

            for _ in 0..1_200 {
                union.insert(Instruction {
                    message_id: thread_rng().next_u32(),
                    property_id: id as u64, 
                    somecnt1: 1,
                    somecnt2: 2,
                    somecnt3: 3,
                    somecnt4: 4
                });
            }
            

            let union = Arc::new(union);
            
            for t in &transmitters_from_unionizer {
                t.1.send(union.clone());
            }
        }
    });
}

async fn accept_peer(ws_stream: WebSocketStream<tokio::net::TcpStream>, transmitter_to_unionizer: mpsc::UnboundedSender<MessageToUnionizer>, address: String, mut receiver_from_unionizer: mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>) {
    
    let (write, read) = ws_stream.split();
    
    receive_instructions(1, WebSocketReader::NonTls(read), address.clone(), transmitter_to_unionizer.clone());
    send_instructions(WebSocketWriter::NonTls(write), address, receiver_from_unionizer);
}

async fn connect_to_server(peer_id: i32, address: String, transmitter_to_unionizer: mpsc::UnboundedSender<MessageToUnionizer>, receiver_from_unionizer: mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>) {
    if let Ok((ws_stream, _)) = connect_async(&address).await {
        println!("Connected to server at: {}.", address);
        let (write, read) = ws_stream.split();
        
        receive_instructions(peer_id, WebSocketReader::MaybeTls(read), address.clone(), transmitter_to_unionizer.clone());
        send_instructions(WebSocketWriter::MaybeTls(write), address.clone(), receiver_from_unionizer);
    }
}

fn receive_instructions(peer_id: i32, mut read: WebSocketReader, address: String, transmitter_to_unionizer: mpsc::UnboundedSender<MessageToUnionizer>) {
    tokio::spawn(async move {
        loop {
            let message = read.next().await;
            handle_received_instructions_wrapped(peer_id, message, address.clone(), &transmitter_to_unionizer);
        }
    });
}

fn handle_received_instructions_wrapped(peer_id: i32, instruction_batch: Option<Result<tungstenite::Message, tungstenite::Error>>, address: String, transmitter_to_unionizer: &mpsc::UnboundedSender<MessageToUnionizer>) {
    match instruction_batch {
        Some(value) => {
            match value {
                Err(_err) => {
                    println!("Error");
                },
                Ok(msg) => {
                    match msg {
                        tungstenite::Message::Binary(data) => {
                            match proto::InstructionBatch::decode(&*data) {
                                Ok(received_message) => {
                                    handle_received_instructions(peer_id, received_message, &address, transmitter_to_unionizer);
                                }
                                Err(err) => {
                                    println!("Failed to decode protobuf message from server {}: {}", address, err);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        },
        None => {
            println!("Error");
        }
    }
}

fn handle_received_instructions(peer_id: i32, instruction_batch: InstructionBatch, address: &String, transmitter_to_unionizer: &mpsc::UnboundedSender<MessageToUnionizer>) {
    match verify_instruction_batch(&instruction_batch, address) {
        Ok(_) => {
            // we need to put this into log for later retrieval, maybe into the db as well.
            // we need to send appropriate reply to the sender to acknowledge that message was received and to sign that as well.
            // then we just need to pass this to unionizing thread and our job here is done, right?
            transmitter_to_unionizer.send(MessageToUnionizer{
                instruction_batch,
                peer_id
            });
        },
        Err(_) => {
            // Are you trying to screw me peer??!!
        }
    }
}

fn verify_instruction_batch(instruction_batch: &InstructionBatch, _address: &String) -> Result<(), ed25519_dalek::ed25519::Error> {
    let decoded_public_key = hex::decode("5655055ffb08bf45b3fa33d4efb85d13ddfc4201341c9f8c4ba6782ab7ef05a7").expect("asd");
    let mut decoded_public_key_array: [u8; 32] = [0; 32];
    decoded_public_key_array.clone_from_slice(&decoded_public_key);
    let mut hasher = Sha256::new();

    let mut signature_array : [u8; 64] = [0; 64];
    signature_array.clone_from_slice(&instruction_batch.signature);
    let signature: Signature = Signature::from_bytes(&signature_array);

    for item in &instruction_batch.instructions {
        let content_bytes = format!("{:?}", item.property_id).into_bytes();
        hasher.update(content_bytes);
    }

    let hashed_content = hasher.finalize_reset();
    VerifyingKey::from_bytes(&decoded_public_key_array).expect("There was a problem here.").verify(&hashed_content, &signature)
}

fn send_instructions(mut write: WebSocketWriter, address: String, mut receiver_from_unionizer: mpsc::UnboundedReceiver<Arc<HashSet<Instruction>>>) {
    tokio::spawn(async move {
        let mut allready_sent: HashSet<u128> = HashSet::new();
        loop {
            match receiver_from_unionizer.recv().await {
                Some((received_from_all)) => {
                    let to_send: InstructionBatch = create_message_to_send(received_from_all, &allready_sent);
                    let mut buf = Vec::new();
                    to_send.encode(&mut buf);
                    if write.send(tokio_tungstenite::tungstenite::Message::Binary(buf)).await.is_err() {
                        break;
                    }
                    for sent_instruction in &to_send.instructions {
                        let hash = ((sent_instruction.property_id as u128) << 32) | sent_instruction.message_id as u128;
                        allready_sent.insert(hash);
                    }
                    // println!("Sent to client at address {} {} messages", address, allready_sent.len());
                }
                None => {
                    // TO DO handle this
                }
            }
        }
    });
}

fn create_message_to_send(received_from_all: Arc<HashSet<Instruction>>, allready_sent: &HashSet<u128>) -> InstructionBatch {
    let mut instructions = vec![];
    for ins in received_from_all.iter() {
        let hash = ((ins.property_id as u128) << 32) | ins.message_id as u128;
        if !allready_sent.contains(&hash) {
            instructions.push(ins.clone()); // TO DO - find for the love of god a better way to do this. 
        }
    }

    InstructionBatch {
        signature: sign_instructions(&instructions).to_bytes().to_vec(),
        instructions,
    }
}

fn sign_instructions(instructions: &Vec<Instruction>) -> Signature {
    let decoded_private_key = hex::decode("8ba83dff19af83039514d0ad375209db5fd1fc37713671a5b0fa8999c267299f").expect("asd");
    let mut decoded_private_key_array: [u8; 32] = [0; 32];
    decoded_private_key_array.clone_from_slice(&decoded_private_key);
    let mut hasher = Sha256::new();

    for instruction in instructions {
        let content_bytes = format!("{:?}", instruction.property_id).into_bytes();
        hasher.update(content_bytes);
    }

    let hashed_content = hasher.finalize();
    let mut signing_key: SigningKey = SigningKey::from(decoded_private_key_array);

    signing_key.sign(&hashed_content)
}
