use typeset::*;

/// Basic pretty printing example showing fundamental layout combinators
fn main() {
    println!("=== Basic Layout Combinators ===\n");

    // Basic text layouts
    let hello = text("Hello".to_string());
    let world = text("World".to_string());

    // Unpadded composition (no spaces)
    let hello_world_unpadded = comp(
        hello.clone(),
        world.clone(),
        Pad::Unpadded,
        Break::Breakable,
    );
    println!(
        "Unpadded: \"{}\"",
        render(compile(hello_world_unpadded), 2, 80)
    );

    // Padded composition (with spaces)
    let hello_world_padded = comp(hello.clone(), world.clone(), Pad::Padded, Break::Breakable);
    println!("Padded: \"{}\"", render(compile(hello_world_padded), 2, 80));

    // Line breaks
    let hello_newline_world = line(hello.clone(), world.clone());
    println!(
        "Line break:\n\"{}\"",
        render(compile(hello_newline_world), 2, 80)
    );

    // Nested layouts
    let nested = nest(comp(
        text("Indented".to_string()),
        text("text".to_string()),
        Pad::Padded,
        Break::Breakable,
    ));
    let with_prefix = comp(
        text("Prefix:".to_string()),
        nested,
        Pad::Unpadded,
        Break::Breakable,
    );
    println!(
        "Nested (broken):\n\"{}\"",
        render(compile(with_prefix), 2, 10)
    ); // Force break with small width

    // Fixed layouts (won't break)
    let fixed_comp = fix(comp(
        text("Fixed".to_string()),
        text("together".to_string()),
        Pad::Unpadded,
        Break::Breakable,
    ));
    println!("Fixed: \"{}\"", render(compile(fixed_comp), 2, 5)); // Will overflow

    // Groups (break as unit)
    let group_inner = comp(
        text("grouped".to_string()),
        text("content".to_string()),
        Pad::Padded,
        Break::Breakable,
    );
    let grouped = grp(group_inner);
    let with_group = comp(
        text("Before".to_string()),
        grouped,
        Pad::Padded,
        Break::Breakable,
    );
    println!(
        "Grouped (fits): \"{}\"",
        render(compile(with_group.clone()), 2, 80)
    );
    println!(
        "Grouped (breaks): \"{}\"",
        render(compile(with_group), 2, 10)
    );

    // Sequence (all break if one breaks)
    let seq_inner = comp(
        text("item1".to_string()),
        comp(
            text("item2".to_string()),
            text("item3".to_string()),
            Pad::Padded,
            Break::Breakable,
        ),
        Pad::Padded,
        Break::Breakable,
    );
    let sequenced = seq(seq_inner);
    println!(
        "Sequence (breaks all):\n\"{}\"",
        render(compile(sequenced), 2, 10)
    );

    // Pack (align to first item position)
    let pack_inner = comp(
        text("first".to_string()),
        comp(
            text("second".to_string()),
            text("third".to_string()),
            Pad::Padded,
            Break::Breakable,
        ),
        Pad::Padded,
        Break::Breakable,
    );
    let packed = pack(pack_inner);
    let with_pack = comp(
        text("Start".to_string()),
        packed,
        Pad::Padded,
        Break::Breakable,
    );
    println!("Pack alignment:\n\"{}\"", render(compile(with_pack), 2, 15));
}
