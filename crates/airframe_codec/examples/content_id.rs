use airframe_codec::{content_id_sha256, ContentId};

fn main() {
    let data = b"Hello, Airframe!";
    let cid: ContentId = content_id_sha256(data);
    println!("Content ID (sha256) bytes: {} bytes", cid.as_bytes().len());
    println!("Content ID (hex): {}", cid.to_hex());
}
