import { test } from '@japa/runner'

import {
  ProcessOutputBuffer,
  PROCESS_OUTPUT_TRUNCATION_SUFFIX,
} from '#services/process_output_buffer'

test.group('Process output buffer', () => {
  test('captures full output while under the limit', ({ assert }) => {
    const buffer = new ProcessOutputBuffer(32)

    buffer.append('abc')
    buffer.append(Buffer.from('def'))

    assert.equal(buffer.toString(), 'abcdef')
    assert.isFalse(buffer.isTruncated)
  })

  test('truncates output when the limit is exceeded', ({ assert }) => {
    const buffer = new ProcessOutputBuffer(5)

    buffer.append('hello')
    buffer.append(' world')

    assert.equal(buffer.toString(), `hello${PROCESS_OUTPUT_TRUNCATION_SUFFIX}`)
    assert.isTrue(buffer.isTruncated)
  })

  test('returns only the truncation suffix when no bytes can be captured', ({ assert }) => {
    const buffer = new ProcessOutputBuffer(0)

    buffer.append('hello')

    assert.equal(buffer.toString(), PROCESS_OUTPUT_TRUNCATION_SUFFIX.trimStart())
    assert.isTrue(buffer.isTruncated)
  })
})
