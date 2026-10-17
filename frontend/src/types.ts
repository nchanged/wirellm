export interface ProviderRuntimeDetail {
  name: string;
  enabled: boolean;
  base_url: string;
  api_key_template?: string;
  models: string[];
  circuit: string;
  failures: number;
  rate_limited: boolean;
  rate_limited_until_secs?: number;
  current_requests: number;
  current_successes: number;
  current_input_tokens: number;
  current_output_tokens: number;
  lifetime_requests: number;
  lifetime_successes: number;
  lifetime_input_tokens: number;
  lifetime_output_tokens: number;
  avg_latency_ms: number;
  smartness_score: number;
  max_rpm?: number;
  current_rpm: number;
  is_rpm_capped: boolean;
  min_interval_ms?: number;
  is_burst_limited: boolean;
  in_flight: number;
  breathing_room_secs?: number;
  auto_disabled_reason?: string | null;
  auto_disabled_retry_secs?: number | null;
  has_encrypted_key?: boolean;
  key_preview?: string;
}

export interface VirtualModelTarget {
  provider: string;
  model: string;
}

export interface VirtualModelDetail {
  name: string;
  description: string;
  weight_smartness: number;
  weight_speed: number;
  weight_reliability: number;
  targets: VirtualModelTarget[];
}

export interface ApiStatusResponse {
  uptime_secs: number;
  total_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  providers: ProviderRuntimeDetail[];
  virtual_models: VirtualModelDetail[];
}

export interface ProviderInput {
  name: string;
  enabled: boolean;
  base_url: string;
  api_key_template: string;
  raw_api_key?: string;
  models: string[];
  headers: Record<string, string>;
  rpm?: number;
  min_interval_ms?: number;
  max_concurrency?: number;
  smartness_score?: number;
}

export interface DailyStats {
  date: string;
  provider_name: string;
  model_name: string;
  total_requests: number;
  successful_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  avg_latency_ms: number;
}

export interface AnalyticsResponse {
  days: number;
  from?: string;
  to?: string;
  stats: DailyStats[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_requests: number;
  estimated_cost_saved_usd: number;
}

export interface TestConnectionResponse {
  success: boolean;
  status_code: number;
  latency_ms: number;
  message: string;
  error?: string;
}

export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
  request_id?: string;
  provider?: string;
}

export interface LogsResponse {
  capacity: number;
  count: number;
  entries: LogEntry[];
}

export interface LiveStreamEvent {
  request_id: string;
  virtual_model?: string;
  provider: string;
  model: string;
  delta: string;
  is_done: boolean;
  error?: string;
  tokens_so_far: number;
  elapsed_ms: number;
}

export interface RecentStreamItem {
  event: LiveStreamEvent;
  timestamp_ms: number;
}

export interface SessionUser {
  email: string;
  name: string;
  picture?: string;
  exp: number;
}

export interface AuthStatusResponse {
  google_auth_enabled: boolean;
  api_auth_required: boolean;
  authenticated: boolean;
  user?: SessionUser | null;
  is_cloud_env?: boolean;
}

export interface ApiKeyRecord {
  id: string;
  name: string;
  key_preview: string;
  full_key?: string;
  created_at: string;
}

export interface VaultSecretRecord {
  name: string;
  key_preview: string;
  description: string;
  created_at: string;
  updated_at: string;
}

export interface VaultSecretInput {
  name: string;
  value: string;
  description?: string;
}

export interface DiscoverModelsResponse {
  provider: string;
  base_url: string;
  total_models: number;
  models: string[];
  recommended_coding: string[];
  recommended_reasoning: string[];
  recommended_fast: string[];
}



