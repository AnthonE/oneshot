//! Every effect the fx stage can fire (`look/fx/stage.rs`): `tools/fx.sh list`.

fn main() {
    for s in oneshot_client::look::fx::stage::list() {
        println!("{:<22} {:>4.2} m {:>4.1} s  {}", s.name, s.radius, s.span_s, s.what);
    }
}
