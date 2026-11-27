// Minimal zero-dependency ambient declarations for Node.js built-ins used by
// @decapodlabs/decapod-client and @decapodlabs/pi-decapod so strict `tsc --noEmit`
// succeeds offline without requiring node_modules/@types/node.

interface NodeProcessEnv {
  [key: string]: string | undefined;
}
declare const process: {
  readonly env: NodeProcessEnv;
  readonly execPath: string;
  readonly platform: string;
  readonly arch: string;
  cwd(): string;
};
interface Buffer extends Uint8Array {
  toString(encoding?: string): string;
}
declare const Buffer: {
  concat(list: readonly Uint8Array[]): Buffer;
};

declare module "node:child_process" {
  export interface ChildProcessStream {
    on(event: "data", listener: (chunk: Buffer) => void): void;
  }
  export interface ChildProcessWritable {
    write(chunk: string, encoding?: string): boolean;
    end(): void;
  }
  export interface ChildProcess {
    readonly stdin: ChildProcessWritable;
    readonly stdout: ChildProcessStream;
    readonly stderr: ChildProcessStream;
    kill(signal?: string): boolean;
    on(event: "error", listener: (err: Error) => void): void;
    on(event: "close", listener: (code: number | null) => void): void;
  }
  export function spawn(
    command: string,
    args: readonly string[],
    options?: {
      cwd?: string;
      env?: NodeProcessEnv;
      stdio?: readonly ["pipe", "pipe", "pipe"];
    },
  ): ChildProcess;
}

declare module "node:crypto" {
  export interface Hash {
    update(data: string, inputEncoding?: string): Hash;
    digest(encoding: "hex"): string;
  }
  export function createHash(algorithm: string): Hash;
}

declare module "node:fs" {
  export function existsSync(path: string): boolean;
  export function readFileSync(path: string, encoding: "utf8"): string;
}

declare module "node:path" {
  export function resolve(...paths: readonly string[]): string;
  export function dirname(path: string): string;
}

declare module "node:module" {
  export type NodeRequire = (id: string) => unknown;
  export function createRequire(filename: string | URL): NodeRequire;
}

declare module "node:perf_hooks" {
  export const performance: {
    now(): number;
  };
}

declare module "node:assert/strict" {
  const assert: {
    equal<T>(actual: unknown, expected: T, message?: string): asserts actual is T;
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    ok(value: unknown, message?: string): asserts value;
    rejects(
      promise: Promise<unknown>,
      predicate?: (err: unknown) => boolean,
    ): Promise<void>;
  };
  export default assert;
}

declare module "node:test" {
  export default function test(
    name: string,
    fn: () => void | Promise<void>,
  ): void;
}
