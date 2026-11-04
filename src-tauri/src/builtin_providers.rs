//! Generated from the upstream `src/main/models-config.ts` BUILTIN_PROVIDERS set.
//! Regenerate with the extraction snippet in docs/handoff.md instead of editing by hand.

pub const BUILTIN_PROVIDERS: &[&str] = &[
    "abliteration",
    "aiand",
    "aimlapi",
    "alibaba-coding-plan",
    "alibaba-token-plan",
    "amazon-bedrock",
    "anthropic",
    "azure",
    "baseten",
    "bedrock-mantle",
    "cerebras",
    "charm-hyper",
    "cline-pass",
    "cloudflare-ai-gateway",
    "commandcode",
    "coreweave",
    "cursor",
    "deepinfra",
    "deepseek",
    "devin",
    "exa",
    "firepass",
    "fireworks",
    "github-copilot",
    "gitlab-duo",
    "gitlab-duo-agent",
    "gmi-cloud",
    "google",
    "google-antigravity",
    "google-gemini-cli",
    "google-vertex",
    "groq",
    "huggingface",
    "kagi",
    "kilo",
    "kimi-code",
    "litellm",
    "llama.cpp",
    "lm-studio",
    "local",
    "meta",
    "minimax",
    "minimax-code",
    "minimax-code-cn",
    "mistral",
    "moonshot",
    "muse-code",
    "nanogpt",
    "novita",
    "nvidia",
    "ollama",
    "ollama-cloud",
    "openai",
    "openai-codex",
    "openai-codex-device",
    "opencode-go",
    "opencode-zen",
    "openrouter",
    "parallel",
    "perplexity",
    "qianfan",
    "qwen-portal",
    "sakana",
    "siliconflow",
    "siliconflow-cn",
    "singularityapi",
    "stencil",
    "synthetic",
    "tavily",
    "together",
    "typesafe",
    "umans",
    "venice",
    "vercel-ai-gateway",
    "vllm",
    "wafer-serverless",
    "web",
    "xai",
    "xai-oauth",
    "xiaomi",
    "xiaomi-token-plan-ams",
    "xiaomi-token-plan-cn",
    "xiaomi-token-plan-sgp",
    "yolo-auto",
    "zai",
    "zai-coding-plan",
    "zenmux",
    "zhipu-coding-plan",
];

pub fn is_builtin(id: &str) -> bool {
    BUILTIN_PROVIDERS.binary_search(&id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_matches_upstream() {
        assert_eq!(BUILTIN_PROVIDERS.len(), 88);
    }

    #[test]
    fn list_is_sorted_for_binary_search() {
        assert!(BUILTIN_PROVIDERS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn known_ids_resolve() {
        assert!(is_builtin("anthropic"));
        assert!(is_builtin("zai"));
        assert!(!is_builtin("my-private-endpoint"));
    }
}
