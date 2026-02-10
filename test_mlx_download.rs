use stt::adapters::mlx_parakeet::SharedMlxParakeetAdapter;
use stt::SttConfig;

#[tokio::main]
async fn main() {
    let adapter = SharedMlxParakeetAdapter::new();
    let config = SttConfig {
        model_name: "mlx-community/parakeet-tdt-0.6b-v3".to_string(),
        ..Default::default()
    };
    
    println!("Initializing MLX adapter with model: {}", config.model_name);
    match adapter.initialize(config).await {
        Ok(_) => println!("Success: MLX adapter initialized and model ready."),
        Err(e) => println!("Error: Failed to initialize MLX adapter: {}", e),
    }
}
