// 模拟 llmApi：getConfig 返回可变的「后端配置」快照
let cfg = {
  providers: [
    {
      id: 'p1',
      name: '测试提供方',
      provider_type: 'openai',
      base_url: 'https://example.com/v1',
      api_key: 'sk-test',
      default_model: 'm1',
      models: ['m1'],
      enabled: true,
      input_price_per_1m: 0,
      output_price_per_1m: 0,
      monthly_token_limit: null,
      monthly_cost_limit: null,
      extra_headers: {},
      model_meta: {},
      created_at: '',
      updated_at: '',
    },
  ],
  default_provider_id: 'p1',
  last_chat_provider_id: null,
  last_chat_model: null,
};

export function __setConfig(next) {
  cfg = JSON.parse(JSON.stringify(next));
}

export const llmApi = {
  getConfig: async () => structuredClone(cfg),
};
