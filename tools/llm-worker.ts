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

import { generateText } from 'ai'
import { claudeCode } from 'ai-sdk-provider-claude-code'
import pMap from 'p-map'
import type { z } from 'zod'

// Ensure a clean Claude Code environment.
delete process.env.CLAUDECODE

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
      const { text } = await generateText({
        model: claudeCode(job.model, {
          permissionMode: 'bypassPermissions',
          allowedTools: [],
          maxTurns: 1,
        }),
        prompt: job.prompt,
      })

      // Extract JSON from the response — the LLM sometimes wraps
      // it in markdown fences or adds prose around it.
      const jsonMatch = text.match(/\{[\s\S]*\}/)
      if (!jsonMatch) {
        return { id: job.id, error: 'no JSON object in LLM response' }
      }

      return { id: job.id, result: jsonMatch[0] }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      const isTransient =
        message.includes('timeout') ||
        message.includes('rate') ||
        message.includes('ECONNRESET') ||
        message.includes('503') ||
        message.includes('529') ||
        message.includes('overloaded')

      if (isTransient && attempt < MAX_RETRIES) {
        const delay = RETRY_DELAYS_MS[attempt] ?? 8_000
        process.stderr.write(
          `[retry] ${job.id} attempt ${attempt + 1}/${MAX_RETRIES} — ${message.slice(0, 80)}\n`,
        )
        await Bun.sleep(delay)
        continue
      }

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
