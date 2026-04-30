export const PROCESS_OUTPUT_TRUNCATION_SUFFIX = '\n...[saida truncada pelo limite de captura]'

export class ProcessOutputBuffer {
  private readonly chunks: Buffer[] = []
  private bytesCaptured = 0
  private truncated = false

  constructor(
    private readonly maxBytes = 256 * 1024,
    private readonly truncationSuffix = PROCESS_OUTPUT_TRUNCATION_SUFFIX
  ) {}

  append(chunk: Buffer | string): void {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)

    if (!buffer.length) {
      return
    }

    const remainingBytes = this.maxBytes - this.bytesCaptured

    if (remainingBytes <= 0) {
      this.truncated = true
      return
    }

    if (buffer.length <= remainingBytes) {
      this.chunks.push(buffer)
      this.bytesCaptured += buffer.length
      return
    }

    this.chunks.push(buffer.subarray(0, remainingBytes))
    this.bytesCaptured += remainingBytes
    this.truncated = true
  }

  toString(): string {
    const output = Buffer.concat(this.chunks, this.bytesCaptured).toString('utf8')

    if (!this.truncated) {
      return output
    }

    if (!output) {
      return this.truncationSuffix.trimStart()
    }

    return output.endsWith('\n')
      ? `${output}${this.truncationSuffix.trimStart()}`
      : `${output}${this.truncationSuffix}`
  }

  get isTruncated(): boolean {
    return this.truncated
  }
}
