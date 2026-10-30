import type { ComputedRef, MaybeRefOrGetter } from 'vue';

export type ReactiveParams<Params> = MaybeRefOrGetter<{ [Key in keyof Params]: MaybeRefOrGetter<Params[Key] | null | undefined> }>;

export interface VueQueryOptions<Result> {
  queryKey: ComputedRef<readonly unknown[]>;
  enabled: ComputedRef<boolean>;
  queryFn: (context: { signal: AbortSignal }) => Promise<Result>;
}

export declare function withVueQueryOptions<Call extends { operation: { service: string; id: string; method: string } }>(call: Call, execute: object): Call;
