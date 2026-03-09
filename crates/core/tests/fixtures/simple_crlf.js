function greet(name) {
  const message = `hello, ${name}`
  return message
}

export function main() {
  const name = 'world'
  return greet(name)
}
