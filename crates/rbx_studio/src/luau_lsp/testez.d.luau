-- The globals the Tests dock's runner gives a spec (`rbx_lua::test_runner`):
-- TestEZ's names plus the Jest-Lua matchers it takes beside them.

type TestEZBody = (...any) -> ()
type TestEZBlock = ((phrase: string, body: TestEZBody) -> ()) & {
	skip: (phrase: string, body: TestEZBody) -> (),
	only: (phrase: string, body: TestEZBody) -> (),
}

type TestEZExpectation = {
	to: TestEZExpectation,
	be: TestEZExpectation,
	been: TestEZExpectation,
	have: TestEZExpectation,
	was: TestEZExpectation,
	at: TestEZExpectation,
	never: TestEZExpectation,
	equal: (expected: any) -> (),
	ok: () -> (),
	a: (typeName: string) -> (),
	an: (typeName: string) -> (),
	near: (expected: number, limit: number?) -> (),
	throw: (fragment: string?) -> (),
	toBe: (expected: any) -> (),
	toEqual: (expected: any) -> (),
	toBeTruthy: () -> (),
	toBeFalsy: () -> (),
	toBeNil: () -> (),
	toBeCloseTo: (expected: number, digits: number?) -> (),
	toThrow: (fragment: string?) -> (),
	toContain: (item: any) -> (),
	toHaveLength: (length: number) -> (),
	toBeGreaterThan: (other: number) -> (),
	toBeGreaterThanOrEqual: (other: number) -> (),
	toBeLessThan: (other: number) -> (),
	toBeLessThanOrEqual: (other: number) -> (),
}

declare describe: TestEZBlock
declare it: TestEZBlock
declare test: TestEZBlock
declare function xdescribe(phrase: string, body: TestEZBody): ()
declare function fdescribe(phrase: string, body: TestEZBody): ()
declare function describeSKIP(phrase: string, body: TestEZBody): ()
declare function describeFOCUS(phrase: string, body: TestEZBody): ()
declare function xit(phrase: string, body: TestEZBody): ()
declare function fit(phrase: string, body: TestEZBody): ()
declare function itSKIP(phrase: string, body: TestEZBody): ()
declare function itFOCUS(phrase: string, body: TestEZBody): ()
declare function SKIP(): ()
declare function FOCUS(): ()
declare function beforeAll(body: TestEZBody): ()
declare function afterAll(body: TestEZBody): ()
declare function beforeEach(body: TestEZBody): ()
declare function afterEach(body: TestEZBody): ()
declare function expect(value: any): TestEZExpectation
