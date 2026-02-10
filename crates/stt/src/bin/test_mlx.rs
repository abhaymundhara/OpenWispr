use stt::create_adapter;
use stt::SttConfig;

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let mut adapter = create_adapter().expect("Failed to create adapter");
        let model_name = "mlx-community/parakeet-tdt-0.6b-v3";
        
        println!("Initializing adapter with model: {}", model_name);
        match adapter.initialize(SttConfig {
            model_name: model_name.to_string(),
            ..Default::default()
        }).await {
            Ok(_) => println!("Success: Adapter initialized and model ready."),
            Err(e) => println!("Error: Failed to initialize adapter: {}", e),
        }
    });
}
