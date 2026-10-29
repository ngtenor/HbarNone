import effLongWordList from './data/eff-large-wordlist.txt?raw'

const EFF_WORDS = effLongWordList.trim().split(/\s+/)
const WORD_COUNT = 6
const RANDOM_RANGE = 0x1_0000_0000

export function generatePassphrase(): string {
  if (EFF_WORDS.length !== 7776) {
    throw new Error('EFF word list must contain exactly 7776 words')
  }

  const limit = Math.floor(RANDOM_RANGE / EFF_WORDS.length) * EFF_WORDS.length
  const randomValue = new Uint32Array(1)
  const words: string[] = []

  while (words.length < WORD_COUNT) {
    crypto.getRandomValues(randomValue)
    const value = randomValue[0]
    if (value >= limit) continue

    words.push(EFF_WORDS[value % EFF_WORDS.length])
  }

  return words.join(' ')
}
