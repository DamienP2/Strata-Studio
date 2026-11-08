//! Try the host's ear on a recording.
//!
//! ```text
//! cargo run -p strata-studio-sense --example ecoute -- enregistrement.wav [127.0.0.1:8178]
//! ```
//!
//! A whisper.cpp server must be running on that address with a
//! multilingual model, for example:
//!
//! ```text
//! whisper-server -m models/ggml-base.bin --host 127.0.0.1 --port 8178 -l fr
//! ```
//!
//! The recording is the host's microphone (16-bit PCM WAV, any rate, the
//! first channel is used). It is fed as it would come live, 20 ms at a
//! time; each sentence heard is printed, with what the Director's listener
//! understands of it. The names known are those of the example studio:
//! Paul (host), Alice, Karim on the set, Julie in duplex, Marc on a call.

use std::net::SocketAddr;
use strata_studio_core::{Listener, StudioSetup};
use strata_studio_sense::ear::HostEar;
use strata_studio_sense::wav;
use strata_studio_sense::whisper::WhisperServer;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage : ecoute <enregistrement.wav> [adresse du serveur whisper]");
        std::process::exit(2);
    };
    let addr: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:8178".into())
        .parse()
        .expect("adresse");
    let bytes = std::fs::read(&path).expect("lecture du fichier");
    let (samples, rate) = wav::decode(&bytes).expect("WAV 16 bits");

    let mut setup = StudioSetup::example();
    for (seat, n) in setup
        .seats
        .iter_mut()
        .zip(["Paul Martin", "Alice Durand", "Karim Benali"])
    {
        seat.guest = Some(n.into());
    }
    setup.remotes[0].guest = Some("Marc Lefèvre".into());
    setup.remotes[1].guest = Some("Julie Roux".into());
    let listener = Listener::new(&setup);
    let host = setup.host_seat().expect("un animateur");

    let mut ear = HostEar::new(host, rate, WhisperServer::new(addr));
    let step = (rate / 50) as usize;
    let mut heard = Vec::new();
    for chunk in samples.chunks(step) {
        heard.extend(ear.hear(chunk));
    }
    heard.extend(ear.finish());
    if heard.is_empty() {
        println!("Rien entendu (le serveur whisper répond-il sur {addr} ?)");
    }
    for h in heard {
        let mark = if h.is_final { "" } else { " (en cours)" };
        println!("{:>7.2} s  « {} »{mark}", h.t_ms as f64 / 1000.0, h.text);
        for hint in listener.hear(&h) {
            println!(
                "           → {:?}, {:?}, confiance {:.0} %, mise en page {:?}",
                hint.target,
                hint.when,
                hint.confidence * 100.0,
                hint.layout
            );
        }
    }
}
