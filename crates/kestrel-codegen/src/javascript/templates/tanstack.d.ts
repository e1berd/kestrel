import type { Operation, Parameters, RequestOptions } from './web.js';

export interface QueryOptions<Result> {
  queryKey: readonly unknown[];
  queryFn: (context: { signal: AbortSignal }) => Promise<Result>;
}

export interface MutationOptions<Parameters, Result> {
  mutationKey: readonly unknown[];
  mutationFn: (params: Parameters) => Promise<Result>;
}

export declare function queryValue(value: unknown, ancestors?: Set<unknown>): unknown;
export declare function queryResult(result: unknown): Promise<unknown>;
export declare function operationKey(execute: object, operation: Operation): readonly unknown[];
export declare function requestSettings(options: RequestOptions): { signal: AbortSignal | undefined; settings: Record<string, unknown> };
export declare function snapshotParams(params: Parameters): unknown;
export declare function withQueryOptions<Call extends { operation: { service: string; id: string; method: string } }>(call: Call, execute: object): Call;
