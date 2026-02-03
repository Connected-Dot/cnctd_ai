pub mod config;
pub mod options;
mod anthropic;
mod openai;
mod openai_responses;
mod gemini;

pub use config::{AnthropicConfig, OpenAiConfig, GeminiConfig};
pub use options::ClientOptions;

use crate::batch::{BatchItem, BatchInfo, BatchResult, BatchAwaitOptions, BatchStatus};
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct Client {
    provider: ProviderType,
    options: ClientOptions,
}

impl Client {
    pub fn anthropic(
        config: AnthropicConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self> {
        let options = options.unwrap_or_default();

        Ok(Self {
            provider: ProviderType::Anthropic { config },
            options,
        })
    }

    pub fn openai(
        config: OpenAiConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self> {
        let options = options.unwrap_or_default();

        // Create the OpenAI config
        let mut openai_config = async_openai::config::OpenAIConfig::new()
            .with_api_key(&config.api_key);

        if let Some(org) = &config.organization {
            openai_config = openai_config.with_org_id(org);
        }

        if let Some(base_url) = &options.base_url {
            openai_config = openai_config.with_api_base(base_url);
        }

        let sdk_client = async_openai::Client::with_config(openai_config);

        Ok(Self {
            provider: ProviderType::OpenAi {
                sdk_client,
                config,
            },
            options,
        })
    }

    pub fn gemini(
        config: GeminiConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self> {
        let options = options.unwrap_or_default();

        Ok(Self {
            provider: ProviderType::Gemini { config },
            options,
        })
    }

    pub async fn complete(
        &self,
        request: crate::request::CompletionRequest,
    ) -> Result<crate::response::CompletionResponse> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                anthropic::complete(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                // Use Responses API for all OpenAI models
                openai_responses::complete(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                gemini::complete(config, &request).await
            }
        }
    }

    pub async fn complete_stream(
        &self,
        request: crate::request::CompletionRequest,
    ) -> Result<crate::stream::CompletionStream> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                anthropic::stream(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                // Use Responses API for all OpenAI models
                openai_responses::stream(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                gemini::stream(config, &request).await
            }
        }
    }

    // =========================================================================
    // Video Analysis API methods
    // =========================================================================

    /// Analyze video content using vision-capable models.
    ///
    /// Supported by:
    /// - **Gemini**: Native video support with File API, inline data, and YouTube URLs
    /// - **OpenAI**: Frame-based analysis (requires pre-extracted frames)
    /// - **Anthropic**: Not supported
    ///
    /// # Example (Gemini - native video)
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{Client, VideoAnalysisRequest};
    ///
    /// // Analyze a local video file
    /// let request = VideoAnalysisRequest::new("video.mp4", "Describe what happens in this video");
    /// let response = client.analyze_video(request).await?;
    ///
    /// // Analyze a YouTube video
    /// let request = VideoAnalysisRequest::new(
    ///     "https://www.youtube.com/watch?v=abc123",
    ///     "Summarize the main points"
    /// );
    ///
    /// // With video clipping and frame rate options
    /// let request = VideoAnalysisRequest::new("video.mp4", "What happens?")
    ///     .with_fps(2.0)
    ///     .with_start_offset(30.0)
    ///     .with_end_offset(60.0);
    /// ```
    ///
    /// # Example (OpenAI - frame-based)
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{Client, VideoAnalysisRequest, ImageContent};
    ///
    /// // Extract frames externally (e.g., ffmpeg -i video.mp4 -vf "fps=2" frame_%04d.jpg)
    /// let frames: Vec<ImageContent> = /* load extracted frames */;
    /// let request = VideoAnalysisRequest::from_frames(frames, "Describe the video");
    /// let response = client.analyze_video(request).await?;
    /// ```
    pub async fn analyze_video(
        &self,
        request: crate::video::VideoAnalysisRequest,
    ) -> Result<crate::video::VideoAnalysisResponse> {
        match &self.provider {
            ProviderType::Gemini { config } => {
                crate::video::gemini_analyze(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                crate::video::openai_analyze(sdk_client, config, &request).await
            }
            ProviderType::Anthropic { .. } => {
                Err(Error::UnsupportedOperation(
                    "Anthropic does not support video analysis - use Gemini or OpenAI client".to_string()
                ))
            }
        }
    }

    // =========================================================================
    // Text-to-Speech API methods
    // =========================================================================

    /// Generate speech from text.
    ///
    /// Supported by:
    /// - **OpenAI**: tts-1, tts-1-hd, gpt-4o-mini-tts models
    /// - **Gemini**: gemini-2.5-flash-preview-tts, gemini-2.5-pro-preview-tts
    /// - **Anthropic**: Not supported
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{Client, SpeechRequest, Voice, AudioFormat};
    ///
    /// // Simple TTS
    /// let request = SpeechRequest::new("Hello, world!");
    /// let response = client.generate_speech(request).await?;
    /// response.save("hello.mp3").await?;
    ///
    /// // With options
    /// let request = SpeechRequest::new("Welcome to the future!")
    ///     .with_voice(Voice::Nova)
    ///     .with_speed(1.2)
    ///     .with_format(AudioFormat::Mp3);
    /// let response = client.generate_speech(request).await?;
    ///
    /// // With style instructions (OpenAI gpt-4o-mini-tts only)
    /// let request = SpeechRequest::new("Your order has been shipped!")
    ///     .with_instructions("Speak cheerfully like a friendly customer service agent")
    ///     .with_model("gpt-4o-mini-tts");
    /// ```
    pub async fn generate_speech(
        &self,
        request: crate::tts::SpeechRequest,
    ) -> Result<crate::tts::SpeechResponse> {
        match &self.provider {
            ProviderType::OpenAi { sdk_client, config } => {
                crate::tts::openai_generate(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                crate::tts::gemini_generate(config, &request).await
            }
            ProviderType::Anthropic { .. } => {
                Err(Error::UnsupportedOperation(
                    "Anthropic does not support text-to-speech - use OpenAI or Gemini client".to_string()
                ))
            }
        }
    }

    // =========================================================================
    // Image Generation API methods
    // =========================================================================

    /// Generate images from text prompts.
    ///
    /// Supported by:
    /// - **Gemini**: Native image generation via Nano Banana models
    /// - **OpenAI**: GPT Image models (gpt-image-1, gpt-image-1.5)
    /// - **Anthropic**: Not supported
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{Client, ImageGenerationRequest, AspectRatio, ImageQuality};
    ///
    /// // Simple generation
    /// let request = ImageGenerationRequest::new("A sunset over mountains");
    /// let response = client.generate_image(request).await?;
    ///
    /// // Save the first image
    /// if let Some(image) = response.first() {
    ///     image.save("sunset.png").await?;
    /// }
    ///
    /// // With options
    /// let request = ImageGenerationRequest::new("A futuristic cityscape")
    ///     .landscape()
    ///     .high_quality()
    ///     .with_count(2);
    /// let response = client.generate_image(request).await?;
    /// response.save_all("./output", "cityscape").await?;
    /// ```
    pub async fn generate_image(
        &self,
        request: crate::image_gen::ImageGenerationRequest,
    ) -> Result<crate::image_gen::ImageGenerationResponse> {
        match &self.provider {
            ProviderType::Gemini { config } => {
                crate::image_gen::gemini_generate(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                crate::image_gen::openai_generate(sdk_client, config, &request).await
            }
            ProviderType::Anthropic { .. } => {
                Err(Error::UnsupportedOperation(
                    "Anthropic does not support image generation - use Gemini or OpenAI client".to_string()
                ))
            }
        }
    }

    // =========================================================================
    // Transcription API methods
    // =========================================================================

    /// Transcribe audio to text.
    ///
    /// Supported by OpenAI (Whisper) and Gemini providers.
    /// Anthropic does not support audio transcription.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{Client, TranscriptionRequest};
    ///
    /// let request = TranscriptionRequest::new("audio.mp3")
    ///     .with_language("en")
    ///     .with_timestamps();
    ///
    /// let response = client.transcribe(request).await?;
    /// println!("Transcript: {}", response.text);
    /// ```
    pub async fn transcribe(
        &self,
        request: crate::transcription::TranscriptionRequest,
    ) -> Result<crate::transcription::TranscriptionResponse> {
        match &self.provider {
            ProviderType::OpenAi { sdk_client, config } => {
                crate::transcription::openai_transcribe(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                crate::transcription::gemini_transcribe(config, &request).await
            }
            ProviderType::Anthropic { .. } => {
                Err(Error::UnsupportedOperation(
                    "Anthropic does not support audio transcription - use OpenAI or Gemini client".to_string()
                ))
            }
        }
    }

    // =========================================================================
    // Embedding API methods
    // =========================================================================

    /// Generate embeddings for text input.
    ///
    /// Currently only supported by OpenAI provider.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cnctd_ai::{embed_small, EmbeddingRequest};
    ///
    /// // Using convenience function
    /// let response = client.embed(embed_small("Hello, world!")).await?;
    ///
    /// // Using explicit request
    /// let request = EmbeddingRequest::new("Hello, world!", "text-embedding-3-small");
    /// let response = client.embed(request).await?;
    ///
    /// // Access the embedding vector
    /// let vector = &response.embeddings[0].vector;
    /// ```
    pub async fn embed(
        &self,
        request: crate::embeddings::EmbeddingRequest,
    ) -> Result<crate::embeddings::EmbeddingResponse> {
        match &self.provider {
            ProviderType::OpenAi { sdk_client, config } => {
                crate::embeddings::openai_embed(sdk_client, config, &request).await
            }
            ProviderType::Anthropic { .. } => {
                Err(Error::UnsupportedOperation(
                    "Embeddings are not supported by Anthropic - use OpenAI client".to_string()
                ))
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Embeddings are not yet implemented for Gemini".to_string()
                ))
            }
        }
    }

    // =========================================================================
    // Batch API methods
    // =========================================================================

    /// Create a batch of completion requests for asynchronous processing.
    ///
    /// Batch processing offers ~50% cost reduction with a 24-hour SLA.
    /// Use this for high-volume, non-time-sensitive workloads.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let items = vec![
    ///     BatchItem::new("req-1", request1),
    ///     BatchItem::new("req-2", request2),
    /// ];
    /// let batch = client.create_batch(items).await?;
    /// ```
    pub async fn create_batch(&self, items: Vec<BatchItem>) -> Result<BatchInfo> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                crate::batch::anthropic::create_batch(config, items).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                crate::batch::openai::create_batch(sdk_client, config, items).await
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Batch processing is not supported by Gemini".to_string()
                ))
            }
        }
    }

    /// Get the current status of a batch.
    pub async fn get_batch(&self, batch_id: &str) -> Result<BatchInfo> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                crate::batch::anthropic::get_batch(config, batch_id).await
            }
            ProviderType::OpenAi { sdk_client, .. } => {
                crate::batch::openai::get_batch(sdk_client, batch_id).await
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Batch processing is not supported by Gemini".to_string()
                ))
            }
        }
    }

    /// Cancel a batch that is in progress.
    pub async fn cancel_batch(&self, batch_id: &str) -> Result<BatchInfo> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                crate::batch::anthropic::cancel_batch(config, batch_id).await
            }
            ProviderType::OpenAi { sdk_client, .. } => {
                crate::batch::openai::cancel_batch(sdk_client, batch_id).await
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Batch processing is not supported by Gemini".to_string()
                ))
            }
        }
    }

    /// List batches, optionally limited to a certain count.
    pub async fn list_batches(&self, limit: Option<u32>) -> Result<Vec<BatchInfo>> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                crate::batch::anthropic::list_batches(config, limit).await
            }
            ProviderType::OpenAi { sdk_client, .. } => {
                crate::batch::openai::list_batches(sdk_client, limit).await
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Batch processing is not supported by Gemini".to_string()
                ))
            }
        }
    }

    /// Get the results of a completed batch.
    ///
    /// Returns an error if the batch is not yet complete.
    pub async fn get_batch_results(&self, batch_id: &str) -> Result<Vec<BatchResult>> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                crate::batch::anthropic::get_batch_results(config, batch_id).await
            }
            ProviderType::OpenAi { sdk_client, .. } => {
                crate::batch::openai::get_batch_results(sdk_client, batch_id).await
            }
            ProviderType::Gemini { .. } => {
                Err(Error::UnsupportedOperation(
                    "Batch processing is not supported by Gemini".to_string()
                ))
            }
        }
    }

    /// Wait for a batch to complete and return its results.
    ///
    /// Polls the batch status at the specified interval until completion or timeout.
    ///
    /// # Arguments
    ///
    /// * `batch_id` - The ID of the batch to wait for
    /// * `options` - Optional await configuration (poll interval, timeout)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// // Wait with default options (10s poll, 24h timeout)
    /// let results = client.await_batch(&batch.id, None).await?;
    ///
    /// // Wait with custom options
    /// let options = BatchAwaitOptions {
    ///     poll_interval: Duration::from_secs(30),
    ///     timeout: Duration::from_secs(3600), // 1 hour
    /// };
    /// let results = client.await_batch(&batch.id, Some(options)).await?;
    /// ```
    pub async fn await_batch(
        &self,
        batch_id: &str,
        options: Option<BatchAwaitOptions>,
    ) -> Result<Vec<BatchResult>> {
        let opts = options.unwrap_or_default();
        let start = std::time::Instant::now();

        loop {
            let batch = self.get_batch(batch_id).await?;

            match batch.status {
                BatchStatus::Completed => {
                    return self.get_batch_results(batch_id).await;
                }
                BatchStatus::Failed => {
                    return Err(Error::Other(format!(
                        "Batch {} failed",
                        batch_id
                    )));
                }
                BatchStatus::Cancelled => {
                    return Err(Error::Other(format!(
                        "Batch {} was cancelled",
                        batch_id
                    )));
                }
                BatchStatus::Expired => {
                    return Err(Error::Other(format!(
                        "Batch {} expired",
                        batch_id
                    )));
                }
                _ => {
                    // Still in progress
                    if start.elapsed() > opts.timeout {
                        return Err(Error::Other(format!(
                            "Timeout waiting for batch {} after {:?}",
                            batch_id,
                            opts.timeout
                        )));
                    }
                    tokio::time::sleep(opts.poll_interval).await;
                }
            }
        }
    }
}

#[derive(Clone)]
enum ProviderType {
    Anthropic {
        config: AnthropicConfig,
    },
    OpenAi {
        sdk_client: async_openai::Client<async_openai::config::OpenAIConfig>,
        config: OpenAiConfig,
    },
    Gemini {
        config: GeminiConfig,
    },
}
