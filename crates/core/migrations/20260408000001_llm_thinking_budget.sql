-- 添加 thinking_budget 字段支持 Anthropic thinking budget 配置
ALTER TABLE llm_providers ADD COLUMN thinking_budget INTEGER;
