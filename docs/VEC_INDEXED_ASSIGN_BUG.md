# Bug: corrupción de `vec[i64]` por asignación indexada

**Estado:** reproducido en Avenys / Mire v3.15.0 / v3.16.0.
**Severidad:** alta — afecta cualquier escritura por índice sobre vectores
dinámicos (`vec[T]`). Lecturas posteriores devuelven valores erróneos
o desplazados; el vector queda corrupto silenciosamente.

## Síntomas

Un benchmark que crea un `vec[i64]`, lo llena, y luego escribe por índice
con la sintáxis nativa de asignación indexada, pierde el valor escrito y
corrumpe las lecturas. No hay panic ni error de compilación: el resultado
simplemente es incorrecto.

## Caso mínimo reproducible

```mire
module main
load kioto::strings
load kioto::strings::from
load kioto::lists

pub fn main: () {
    set v = [] :vec[i64] mut
    set i = 0 :i64 mut
    while i < 10 {
        set v = lists::push(v 1)
        set i = i + 1
    }
    set v at 4 = 42          # asignación indexada nativa
    set a0 = lists::get(v 0)
    set a4 = lists::get(v 4)
    set a9 = lists::get(v 9)
    use dasu("esperado: 1 42 1")
    use dasu("leido:    " + strings::from::i64(a0) + " "
            + strings::from::i64(a4) + " " + strings::from::i64(a9))
}
```

**Salida observada:** `leido: 1 1 1` (el 42 se perdió; el vector está corrupto).
**Salida esperada:** `leido: 1 42 1`.

## Workaround (funciona)

Usar `lists::set(vec, idx, val)` de kioto en vez de la asignación
indexada nativa. Esta función enruta a `rt_lists_set_i64` en el runtime C
y escribe el slot correcto sin corromper el vector.

```mire
    lists::set(v 4 42)    # correcto: escribe el slot 4
```

Con `lists::set` la salida es `leido: 1 42 1` (correcto).

## Alcance

- **Afecta:** `vec[T]` dinámico con asignación por índice (`set vec at idx = val`).
- **No afecta:** `arr[T N]` (array de tamaño fijo) — su asignación
  por índice (`set self.data at idx = v`) funciona correctamente.
- **No afecta:** lectura por índice (`lists::get`) ni `lists::push`.
- El bug es de codegen/ABI de la asignación indexada sobre el header
  del vector dinámico (el slot escrito no se resuelve contra el puntero
  base correcto, o se desincroniza el `len`/capacidad).

## Relación con la stress suite

La suite de benchmarks (`Arch/stress`) evita este bug usando `lists::get`
para lectura y acumulando resultados en escalares; los pocos benches que
necesitaban escritura por índice fueron reescritos para no usar la
sintáxis nativa. Por eso la suite actual no lo dispara, pero el bug
sigue presente en el runtime y debe documentarse para cualquier código
que escriba en `vec[]` por índice.
