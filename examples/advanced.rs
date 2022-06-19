use std::sync::{mpsc, Arc};

fn main() {
    let host1 = "example.com";
    let port1 = 1234;

    let host2 = "example.net";
    let port2 = 5678;

    let call = "INVALID";

    let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
        Arc::new(|spot| println!("{}", spot.to_json()));

    let mut sender: Vec<mpsc::Sender<()>> = Vec::new();
    let (tx1, rx1) = mpsc::channel();
    sender.push(tx1);
    let (tx2, rx2) = mpsc::channel();
    sender.push(tx2);

    dxclrecorder::record(host1.into(), port1, call.into(), handler.clone(), rx1);
    dxclrecorder::record(host2.into(), port2, call.into(), handler, rx2);

    ctrlc::set_handler(move || {
        println!("Ctrl-C caught");
        for tx in sender.iter() {
            tx.send(()).expect("Failed to send signal on channel");
        }
        sender.clear();
    })
    .expect("Failed to listen on Ctrl-C");
}
