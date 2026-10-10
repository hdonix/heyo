export const greetings = ["Hello", "Hallo", "Bonjour"];

export function greet(name: string) {
  return `${greetings[0]}, ${name}`;
}
