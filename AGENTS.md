# Flux de Fichiers

## Stack

Node + Fastify + TypeScript + Prisma

J'ai choisi api-node avec Node, Fastify et Prisma car cela permet de construire un backend robuste et scalable, adapté à la gestion concurrente de fichiers et à la coordination entre plusieurs nœuds. Ce choix facilite la mise en place d'une API performante et la gestion des bases de données, tout en restant simple à déployer localement pour un développement rapide.

Do not change the stack without being asked. It was chosen with the user before
the first line of code.

## Security rules

- Les communications entre nœuds doivent être sécurisées par TLS pour éviter toute interception.
- Les accès à l'API doivent être authentifiés et autorisés pour éviter les usages non autorisés.
- Les données sensibles ne doivent jamais être stockées en clair dans la base de données.
- Les erreurs et accès doivent être journalisés localement pour audit et débogage.

## Run contract

`npm run dev` must start the project on http://localhost:3000. Keep that true: it is the command the preview button runs.

## Conventions

- Small files, one responsibility each. Split a file before it becomes hard to read.
- Explicit names over clever ones. No magic numbers.
- Never commit a secret. Anything sensitive goes in `.env` and stays out of git.
- Say what you are about to do in two lines before writing code.
