import { onBeforeUnmount } from 'vue'

export function useDebouncedFn<TArgs extends unknown[]> (
  fn: (...args: TArgs) => void,
  delayMs: number,
): (...args: TArgs) => void {
  let timer: ReturnType<typeof setTimeout> | undefined

  onBeforeUnmount(() => {
    if (timer) clearTimeout(timer)
  })

  return (...args: TArgs) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => fn(...args), delayMs)
  }
}

