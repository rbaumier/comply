/**
 * LLM worker — receives lint jobs on stdin (JSON array), evaluates
 * each via the Vercel AI SDK + claude-code provider in parallel,
 * and streams results as NDJSON on stdout.
 *
 * Protocol:
 *   stdin  → JSON array of { id, prompt, model }
 *   stdout → one JSON line per result: { id, result } or { id, error }
 *   stderr → progress messages for the parent process
 */

import { generateObject } from 'ai'
import { createClaudeCode } from 'ai-sdk-provider-claude-code'
import pMap from 'p-map'
import { z } from 'zod'

// Kill all child processes on exit — prevents zombie claude processes
// when the worker is killed by the parent (SIGTERM) or times out.
function killChildren() {
  try {
    // Kill our entire process group.
    process.kill(-process.pid, 'SIGTERM')
  } catch {
    // Ignore — we may already be dying.
  }
}
process.on('SIGTERM', () => { killChildren(); process.exit(143) })
process.on('SIGINT', () => { killChildren(); process.exit(130) })
process.on('exit', killChildren)

// Ensure a clean Claude Code environment.
delete process.env.CLAUDECODE

const claudeCode = createClaudeCode()

const LintResultSchema = z.object({
  comment_quality: z.object({
    issues: z.array(z.object({
      line: z.number().describe('Line number in source'),
      criterion: z.string().describe('Which criterion was violated'),
      explanation: z.string().describe('Why this is a problem'),
      suggestion: z.string().optional().describe('Suggested rewrite'),
    })),
  }),
  intent_naming: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string().describe('Current function name'),
    suggestion: z.string().optional().describe('Better name'),
  })),
  pii_in_logs: z.array(z.object({
    line: z.number().describe('Line number'),
    fields: z.array(z.string()).describe('PII field names found'),
  })),
  mixed_abstraction: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string(),
    high_level: z.string().optional().describe('High-level operations'),
    low_level: z.string().optional().describe('Low-level details mixed in'),
  })),
  define_errors_out_of_existence: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string(),
    error_condition: z.string().describe('The preventable error condition'),
    redesign: z.string().optional().describe('How to redesign the API'),
  })),
  pull_complexity_downward: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string(),
    pushed_complexity: z.string().describe('What complexity is pushed to callers'),
  })),
  barricade_pattern: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string(),
    explanation: z.string().describe('What validation is scattered'),
  })),
  temporal_decomposition: z.array(z.object({
    line: z.number().describe('Line number'),
    module_or_function: z.string(),
    steps: z.string().describe('The execution-order steps'),
    hidden_decision: z.string().optional().describe('What design decision should define the boundary'),
  })),
  shallow_module: z.array(z.object({
    line: z.number().describe('Line number'),
    function_name: z.string(),
    explanation: z.string().describe('Why this is a shallow wrapper'),
  })),
})


type Job = {
  id: string
  prompt: string
  model: string
}

type SuccessResult = {
  id: string
  result: string
}

type ErrorResult = {
  id: string
  error: string
}

const MAX_RETRIES = 3
const RETRY_DELAYS_MS = [2_000, 4_000, 8_000] as const
const CONCURRENCY = 30

async function processJob(job: Job): Promise<SuccessResult | ErrorResult> {
  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      const { object } = await generateObject({
        model: claudeCode(job.model, {
          permissionMode: 'bypassPermissions',
          allowedTools: [],
          maxTurns: 2,
        }),
        schema: LintResultSchema,
        prompt: job.prompt,
      })

      return { id: job.id, result: JSON.stringify(object) }
    } catch (error) {
      const message = error instanceof Error
        ? `${error.message}${error.cause ? ` (cause: ${error.cause})` : ''}`
        : String(error)
      const isTransient =
        message.includes('timeout') ||
        message.includes('rate') ||
        message.includes('ECONNRESET') ||
        message.includes('503') ||
        message.includes('529') ||
        message.includes('overloaded') ||
        message.includes('CLI returned')

      if (isTransient && attempt < MAX_RETRIES) {
        const delay = RETRY_DELAYS_MS[attempt] ?? 8_000
        process.stderr.write(
          `[retry] ${job.id} attempt ${attempt + 1}/${MAX_RETRIES} — ${message.slice(0, 80)}\n`,
        )
        await Bun.sleep(delay)
        continue
      }

      process.stderr.write(
        `[error] ${job.id} — ${message.slice(0, 300)}\n`,
      )
      return { id: job.id, error: message.slice(0, 200) }
    }
  }

  // Unreachable — the loop always returns.
  return { id: job.id, error: 'exhausted retries' }
}

async function main() {
  const input = await Bun.stdin.text()
  const jobs: readonly Job[] = JSON.parse(input)

  if (jobs.length === 0) {
    process.exit(0)
  }

  process.stderr.write(`comply-llm: ${jobs.length} jobs, concurrency ${CONCURRENCY}\n`)

  let done = 0
  const total = jobs.length

  await pMap(
    jobs,
    async (job) => {
      const result = await processJob(job)
      done++
      process.stderr.write(`\rcomply-llm: [${done}/${total}] ${job.id}`)
      // NDJSON — one JSON line per result.
      process.stdout.write(JSON.stringify(result) + '\n')
    },
    { concurrency: CONCURRENCY },
  )

  process.stderr.write('\n')
}

main().catch((error) => {
  process.stderr.write(`comply-llm: fatal — ${error}\n`)
  process.exit(1)
})
