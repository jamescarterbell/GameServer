#![feature(core_panic)]
#![feature(decl_macro)]
#![feature(proc_macro_hygiene)]

use rocket::*;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::collections::HashSet;
use core::panicking::panic;
use std::sync::Mutex;
use game::*;
use std::fs::OpenOptions;
use std::thread;
use std::time::{Instant, Duration};
use serde::{Deserialize, Serialize};

#[get("/connect")]
fn connect(listeners: State<Mutex<Sender<TcpListener>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0");
    match listener {
        Ok(listener) =>{
            let address =
                listener.local_addr().unwrap().port().to_string();
            listeners.lock().unwrap().send(listener);
            address
        }
        Err(error) =>{
            "00000".to_string()
        }
    }
}

fn recieve_connections_and_send_to_games(receiver: Receiver<TcpListener>, player_stream: Sender<TcpStream>){
    let mut listeners = Vec::<(TcpListener, Instant)>::new();
    loop{
        for listener in receiver.try_iter(){
            listeners.push((listener, Instant::now()));
        }
        listeners = listeners.drain(..).filter(|y|{
            let mut x = &y.0;
            let mut z  = y.1;
            x.set_nonblocking(true);
            match x.accept(){
                Ok((stream, addr)) =>{
                    player_stream.send(stream);
                    println!("Sent!");
                    false
                }
                Err(error) =>{
                    if z.elapsed() >= Duration::from_secs(10){
                        println!("Timeout on port: {}", x.local_addr().unwrap().port());
                        false
                    }
                    else{
                        true
                    }
                }
            }
        }).collect();
    }
}

fn game_starter(player_stream: Receiver<TcpStream>, send_stream: Sender<TcpStream>){
    let mut current_game: Option<(PokerGame, u8)> = None;
    loop{
        for stream in player_stream.try_iter(){
            match current_game{
                Some((mut game, mut players)) =>{
                    stream.set_nonblocking(false);
                    game.add_player(stream);
                    players += 1;
                    println!("Game at {} players", players);
                    current_game = Some((game, players));
                },
                None => {
                    let mut game = PokerGame::new();
                    game.add_player(stream);
                    current_game = Some((game, 1));
                    println!("Made new game!");
                },
            }
        }
        match current_game{
            Some((mut game, mut players)) =>{
                if players == 8{
                    println!("Sending game to run thread!");
                    let cloned_sender = send_stream.clone();
                    thread::spawn(move ||{
                        game_manager(game, cloned_sender);
                    });
                    current_game = None;
                }
                else{
                    current_game = Some((game, players));
                }
            },
            None => {
                let mut game = PokerGame::new();
                current_game = Some((game, 0));
            },
        }
    }
}

fn game_manager(mut poker_game: PokerGame, send_stream: Sender<TcpStream>){
    loop{
        match poker_game.run_game_round(){
            GameStatus::Running => {
                for stream in poker_game.kick_players(){
                    println!("Kicking");
                    let mut de = serde_json::Deserializer::from_reader(stream.try_clone().unwrap());
                    let u = KickedPlayerAction::deserialize(&mut de).unwrap();

                    match u{
                        KickedPlayerAction::Stay => {send_stream.send(stream);},
                        KickedPlayerAction::Leave => {},
                    };
                }},
            GameStatus::Error => {
                for player in poker_game.players.drain(..){};
                break;
            },
            GameStatus::Finished => {
                for stream in poker_game.kick_players(){
                    let mut de = serde_json::Deserializer::from_reader(stream.try_clone().unwrap());
                    let u = KickedPlayerAction::deserialize(&mut de).unwrap();

                    match u{
                        KickedPlayerAction::Stay => {send_stream.send(stream);},
                        KickedPlayerAction::Leave => {},
                    };
                };
                break;
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
enum KickedPlayerAction{
    Stay,
    Leave
}

fn main(){
    println!("{}", serde_json::to_string(&KickedPlayerAction::Stay).unwrap());
    let (stream_sender, stream_reciever) = channel::<TcpStream>();
    let (listener_sender, listener_reciever) = channel::<TcpListener>();
    let mutex_listener_sender = Mutex::new(listener_sender.clone());

    let cloned_sender = stream_sender.clone();
    thread::spawn(move ||{
        recieve_connections_and_send_to_games(listener_reciever, cloned_sender);
    });
    thread::spawn(move ||{
        game_starter(stream_reciever, stream_sender)
    });
    ignite().manage(mutex_listener_sender).mount("/", routes![connect]).launch();
}