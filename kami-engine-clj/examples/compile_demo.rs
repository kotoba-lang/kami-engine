//! Compile a demo Clojure game script to WASM and write it to `demo.wasm`.
//!
//! Run with:
//!   cargo test-native -p kami-clj --example compile_demo --features run

fn main() {
    let src = r#"
(defn init [] 0)

(defn tick [dt]
  (when (key-down? "ArrowRight")
    (play-sound "move"))
  (when (key-down? "ArrowLeft")
    (play-sound "move"))
  (when (key-down? "Space")
    (play-sound "jump"))
  (draw-mesh! "player" (f32 0.0) (f32 0.0) (f32 0.0))
  (delta-ms))
"#;

    let wasm = kami_engine_clj::compile_str_with_prelude(src).expect("compile failed");

    let out = "kami-web/clj-demo.wasm";
    std::fs::write(out, &wasm).expect("write failed");
    println!("wrote {} bytes → {out}", wasm.len());

    // Print WAT-like import summary so the HTML page author knows what to bind.
    println!("\nimports visible to browser host:");
    // Walk the WASM import section (quick scan for module/field names).
    // bytes 8.. skip the 8-byte header, then scan sections.
    let mut i = 8usize;
    while i + 8 < wasm.len() {
        let section_id = wasm[i];
        i += 1;
        let (sec_len, consumed) = leb128_u32(&wasm[i..]);
        i += consumed;
        if section_id == 2 {
            // import section
            let (count, c) = leb128_u32(&wasm[i..]);
            let mut j = i + c;
            for _ in 0..count {
                let (mlen, c) = leb128_u32(&wasm[j..]);
                j += c;
                let module = std::str::from_utf8(&wasm[j..j + mlen as usize]).unwrap_or("?");
                j += mlen as usize;
                let (nlen, c) = leb128_u32(&wasm[j..]);
                j += c;
                let name = std::str::from_utf8(&wasm[j..j + nlen as usize]).unwrap_or("?");
                j += nlen as usize;
                let _desc_kind = wasm[j];
                j += 1; // 0=func, 1=table, 2=mem, 3=global
                let (_type_idx, c) = leb128_u32(&wasm[j..]);
                j += c;
                println!("  [{module}] :: {name}");
            }
            break;
        } else {
            i += sec_len as usize;
        }
    }
}

fn leb128_u32(buf: &[u8]) -> (u32, usize) {
    let mut result = 0u32;
    let mut shift = 0u32;
    let mut i = 0;
    loop {
        let byte = buf[i];
        i += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    (result, i)
}
