addEventListener("DOMContentLoaded", function () {
    // Adjust scaling
    viewport = content.document.createElement("meta");
    viewport.name = "viewport";
    viewport.content = "width=device-width, initial-scale=1";
    content.document.head.appendChild(viewport);

    // Forward event & result to qml
    content.document.body.addEventListener('ccdone', function (event) {
        sendAsyncMessage("Whisperfish:CaptchaDone", { "code": content.document.body.dataset.wfResult });
    });

    // Extract signalcaptcha result
    var sc = content.document.createElement("script");
    sc.textContent = "var wf_cp_done = false;\n" +
    "function wf_cp_handler(c) {\n" +
    "    if (wf_cp_done) {\n" +
    "        return;\n" +
    "    }\n" +
    "    var action = document.location.href.includes('registration') ? 'registration' : 'challenge';\n" +
    "    var token = '';\n" +
    "    var scheme = '';\n" +
    "    var token = '';\n" +
    "    if (grecaptcha.enterprise !== undefined) {\n" +
    "        scheme = 'signal-recaptcha-v2';\n" +
    "        sitekey = '6LfBXs0bAAAAAAjkDyyI1Lk5gBAUWfhI_bIyox5W';\n" +
    "        token = grecaptcha.enterprise.getResponse();\n" +
    "    } else if (document.querySelector('iframe[data-hcaptcha-response]') !== null) {\n" +
    "        scheme = 'signal-hcaptcha';\n" +
    "        sitekey = '5fad97ac-7d06-4e44-b18a-b950b20148ff';\n" +
    "        token = document.querySelector('iframe[data-hcaptcha-response]').getAttribute('data-hcaptcha-response');\n" +
    "    }\n" +
    "    if (token == '') {\n" +
    "        return;\n" +
    "    }\n" +
    "    // Build the captcha string\n" +
    "    wf_cp_done = true;\n" +
    "    var result = 'signalcaptcha://' + [scheme, sitekey, action, token].join('.');\n" +
    "    // Make result accessible and notify frame script\n" +
    "    document.body.dataset.wfResult = result;\n" +
    "    document.body.dispatchEvent(new Event('ccdone'));\n" +
    "    console.log('result');\n" +
    "}\n" +
    "// Watch for body class changes to detect when the capcha is done\n" +
    "var wf_observer = new MutationObserver(wf_cp_handler);\n" +
    "wf_observer.observe(document.body, { 'attributes': true })\n";
    content.document.body.appendChild(sc);
});

