export function formatTotalPercent(value) {
  return String(Math.round(Number(value) || 0))
}

export function formatProcessPercent(value) {
  return (Number(value) || 0).toFixed(1)
}
