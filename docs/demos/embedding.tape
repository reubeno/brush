# embedding.tape - Demonstrate embeddable shell API
# Works with https://github.com/charmbracelet/vhs
# This demo shows the Rust API code (does not execute it)

Output embedding.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 18
Set Theme "Monokai Pro"
Set Width 1600
Set Height 650
Set CursorBlink false

Env PS1 '$ '

Type `# brush provides an embeddable shell API for Rust projects`
Enter
Sleep 1s

Type `cat << 'RUST'`
Enter
Type `use brush_core::Shell;`
Enter
Enter
Type `#[tokio::main]`
Enter
Type `async fn main() -> anyhow::Result<()> {`
Enter
Type `    // Create a shell instance with the builder API`
Enter
Type `    let mut shell = Shell::builder()`
Enter
Type `        .build()`
Enter
Type `        .await?;`
Enter
Enter
Type `    // Run bash commands programmatically`
Enter
Type `    shell.run_string("x=42; echo \"Value: $x\"").await?;`
Enter
Enter
Type `    // Access shell variables from Rust`
Enter
Type `    if let Some(val) = shell.env.get("x") {`
Enter
Type `        println!("Rust sees x = {}", val);`
Enter
Type `    }`
Enter
Enter
Type `    // Define and call shell functions`
Enter
Type `    shell.run_string("greet() { echo \"Hello, $1!\"; }").await?;`
Enter
Type `    shell.invoke_function("greet", &["World"]).await?;`
Enter
Enter
Type `    Ok(())`
Enter
Type `}`
Enter
Type `RUST`
Enter
Sleep 2s

Type ``
Enter
Type `# Use cases: test harnesses, config languages, automation tools`
Enter
Sleep 2s
