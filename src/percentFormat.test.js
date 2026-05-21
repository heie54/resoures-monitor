import test from 'node:test'
import assert from 'node:assert/strict'

import { formatProcessPercent, formatTotalPercent } from './percentFormat.js'

test('total usage percent is shown without decimals', () => {
  assert.equal(formatTotalPercent(12.49), '12')
  assert.equal(formatTotalPercent(12.5), '13')
})

test('process usage percent is shown with one decimal', () => {
  assert.equal(formatProcessPercent(3), '3.0')
  assert.equal(formatProcessPercent(3.14), '3.1')
  assert.equal(formatProcessPercent(3.15), '3.1')
  assert.equal(formatProcessPercent(3.16), '3.2')
})
