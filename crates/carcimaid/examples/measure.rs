//! Print measured text widths — used to validate against mermaid's node sizes.
//! Usage: cargo run -p carcimaid --example measure -- "Start" "Middle" "End"
fn main() {
    for label in std::env::args().skip(1) {
        let w = carcimaid::text::measure_width(&label, 16.0);
        println!("{w:10.4}  {label:?}");
    }
    println!(
        "line_height(16) = {:.4}",
        carcimaid::text::line_height(16.0)
    );
}
