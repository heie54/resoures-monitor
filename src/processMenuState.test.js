import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

import { closeProcessMenuState } from './processMenuState.js'

test('keeps a busy process menu open for normal close attempts', () => {
  const busyMenu = {
    visible: true,
    x: 10,
    y: 20,
    process: { pid: 42, name: 'example.exe' },
    busy: true,
    message: '正在打开位置...'
  }

  assert.equal(closeProcessMenuState(busyMenu), busyMenu)
})

test('force closes a busy process menu after a completed command', () => {
  const busyMenu = {
    visible: true,
    x: 10,
    y: 20,
    process: { pid: 42, name: 'example.exe' },
    busy: true,
    message: '正在打开位置...'
  }

  assert.deepEqual(closeProcessMenuState(busyMenu, { force: true }), {
    visible: false,
    x: 0,
    y: 0,
    process: null,
    busy: false,
    message: ''
  })
})

test('process list and context menu clicks can bubble to close the selection', () => {
  const appVue = readFileSync(new URL('./App.vue', import.meta.url), 'utf8')

  assert.equal(appVue.includes('class="process-list interactive" @click.stop'), false)
  assert.equal(appVue.includes('class="process-context-menu interactive"\n      :style'), true)
  assert.equal(appVue.includes('@click.stop\n      @pointerdown.stop'), false)
})
