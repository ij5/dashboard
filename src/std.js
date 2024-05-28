function print(value) {
    Deno.core.ops.op_println(value.toString() + "\n");
}

const http = {
    get: async (url) => {
        let response = await Deno.core.ops.op_http_get(url);
        return response;
    }
}

