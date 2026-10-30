export interface QueryOptions<Result> {
  queryKey: readonly unknown[];
  queryFn: (context: { signal: AbortSignal }) => Promise<Result>;
}

export interface MutationOptions<Parameters, Result> {
  mutationKey: readonly unknown[];
  mutationFn: (params: Parameters) => Promise<Result>;
}

export declare function withQueryOptions<Call extends { operation: { service: string; id: string; method: string } }>(call: Call, execute: object): Call;
