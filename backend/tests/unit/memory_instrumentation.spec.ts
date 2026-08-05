import { test } from '@japa/runner'

import {
  ContainerMemoryProbe,
  parseCgroupLimit,
  parseCgroupUsage,
  parseInactiveFileBytes,
} from '#services/container_memory_probe'
import { MemoryWatermarkService } from '#services/memory_watermark_service'
import { SystemMonitoringService } from '#services/system_monitoring_service'

test.group('ContainerMemoryProbe - parsing de cgroup', () => {
  test('cgroup v2 sem limite ("max") nao e tratado como limite', ({ assert }) => {
    assert.isNull(parseCgroupLimit('max\n'))
  })

  test('sentinela de "sem limite" do cgroup v1 e ignorada', ({ assert }) => {
    assert.isNull(parseCgroupLimit('9223372036854771712\n'))
  })

  test('limite valido e convertido para bytes', ({ assert }) => {
    assert.equal(parseCgroupLimit('536870912\n'), 536_870_912)
  })

  test('conteudo invalido ou vazio nao vira limite', ({ assert }) => {
    assert.isNull(parseCgroupLimit(''))
    assert.isNull(parseCgroupLimit('   '))
    assert.isNull(parseCgroupLimit('nao-e-numero'))
    assert.isNull(parseCgroupLimit('0'))
    assert.isNull(parseCgroupLimit('-1'))
  })

  test('uso e convertido para bytes', ({ assert }) => {
    assert.equal(parseCgroupUsage('134217728\n'), 134_217_728)
    assert.equal(parseCgroupUsage('0'), 0)
    assert.isNull(parseCgroupUsage('invalido'))
  })

  test('page cache inativo e extraido do memory.stat do cgroup v2', ({ assert }) => {
    const stat = ['anon 104857600', 'file 52428800', 'inactive_file 41943040', 'slab 1048576'].join(
      '\n'
    )

    assert.equal(parseInactiveFileBytes(stat), 41_943_040)
  })

  test('page cache inativo e extraido do memory.stat do cgroup v1', ({ assert }) => {
    const stat = ['cache 52428800', 'rss 104857600', 'total_inactive_file 20971520'].join('\n')

    assert.equal(parseInactiveFileBytes(stat), 20_971_520)
  })

  test('memory.stat sem a chave esperada nao quebra a leitura', ({ assert }) => {
    assert.equal(parseInactiveFileBytes('anon 100\nfile 200'), 0)
    assert.equal(parseInactiveFileBytes(''), 0)
  })
})

test.group('ContainerMemoryProbe - leitura', (group) => {
  group.each.setup(() => {
    ContainerMemoryProbe.reset()
    return () => ContainerMemoryProbe.reset()
  })

  test('retorna uma leitura coerente em qualquer ambiente', ({ assert }) => {
    const reading = ContainerMemoryProbe.read()

    assert.include(['cgroup-v2', 'cgroup-v1', 'os'], reading.source)
    assert.isAbove(reading.totalBytes, 0)
    assert.isAtLeast(reading.usedBytes, 0)
    assert.isAtMost(reading.usedBytes, reading.totalBytes)
    assert.equal(reading.freeBytes, reading.totalBytes - reading.usedBytes)
  })

  test('fora de container a fonte e os.* e nao ha limite de container', ({ assert }) => {
    const reading = ContainerMemoryProbe.read()

    if (reading.source === 'os') {
      assert.isFalse(reading.containerLimited)
      return
    }

    // Dentro de container: o limite so e reportado quando realmente existe.
    assert.isTrue(reading.containerLimited)
  })
})

test.group('MemoryWatermarkService', (group) => {
  group.each.setup(() => {
    MemoryWatermarkService.reset()
    return () => MemoryWatermarkService.reset()
  })

  test('sample registra o primeiro pico', ({ assert }) => {
    MemoryWatermarkService.sample('teste')

    const peaks = MemoryWatermarkService.getPeaks()

    assert.isAbove(peaks.peakRssBytes, 0)
    assert.isAbove(peaks.peakHeapUsedBytes, 0)
    assert.isString(peaks.peakRssObservedAt)
    assert.isString(peaks.peakHeapUsedObservedAt)
  })

  test('picos sao monotonicos entre amostras', ({ assert }) => {
    MemoryWatermarkService.sample('teste')
    const first = MemoryWatermarkService.getPeaks()

    MemoryWatermarkService.sample('teste')
    const second = MemoryWatermarkService.getPeaks()

    assert.isAtLeast(second.peakRssBytes, first.peakRssBytes)
    assert.isAtLeast(second.peakHeapUsedBytes, first.peakHeapUsedBytes)
  })

  test('reset zera a janela de observacao', ({ assert }) => {
    MemoryWatermarkService.sample('teste')
    MemoryWatermarkService.reset()

    const peaks = MemoryWatermarkService.getPeaks()

    assert.equal(peaks.peakRssBytes, 0)
    assert.equal(peaks.peakHeapUsedBytes, 0)
    assert.isNull(peaks.peakRssObservedAt)
    assert.isNull(peaks.peakHeapUsedObservedAt)
  })

  test('limite de heap do V8 e exposto para calcular pressao', ({ assert }) => {
    assert.isAbove(MemoryWatermarkService.getHeapLimitBytes(), 0)
  })
})

test.group('SystemMonitoringService - instrumentacao de memoria', () => {
  test('metricas de memoria declaram a origem do numero', async ({ assert }) => {
    const metrics = await SystemMonitoringService.getResourceMetrics()

    assert.include(['cgroup-v2', 'cgroup-v1', 'os'], metrics.memory.source)
    assert.isBoolean(metrics.memory.containerLimited)
    assert.isAbove(metrics.memory.totalBytes, 0)
    assert.isAtMost(metrics.memory.usagePercent, 100)
  })
})
