use anyhow::Result;
mod api;

fn main() -> Result<()> {
    let client = api::ApiClient::new()?;
    let run = client.get_run(52843050)?;
    println!("{:#?}", run);
    Ok(())
}
