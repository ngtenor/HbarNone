//! Shows how a mirror-node `ProtobufEncoded` account key reads as a key
//! structure. Run with the key hex as the first argument; oversized or
//! unsupported structures are rejected with the message the app would show.

use hedera_tx::key_tree::KeyTree;

fn main() {
    let hex = std::env::args().nth(1).expect("key hex argument");
    match KeyTree::from_mirror("ProtobufEncoded", &hex) {
        Ok(tree) => {
            let (required, total) = tree.required_and_total();
            println!("parsed: {required} of {total}");
            println!("{}", tree.describe());
        }
        Err(message) => println!("rejected: {message}"),
    }
}
