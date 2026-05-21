export function createClosedProcessMenuState() {
  return {
    visible: false,
    x: 0,
    y: 0,
    process: null,
    busy: false,
    message: ''
  }
}

export function closeProcessMenuState(menu, options = {}) {
  if (menu?.busy && !options.force) {
    return menu
  }

  return createClosedProcessMenuState()
}
