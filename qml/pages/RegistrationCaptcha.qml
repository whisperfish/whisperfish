import QtQuick 2.2
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import Nemo.Configuration 1.0
import Nemo.DBus 2.0

// Warning: Do not use this page within Whisperfish:
// Clashing sqlite and sqlcipher causes mozembedded to crash.

WebViewPage {
    id: page
    objectName: "registrationCaptchaPage"

    allowedOrientations: Orientation.PortraitMask

    backNavigation: false
    forwardNavigation: false
    showNavigationIndicator: false
    property bool captchaCaptured: false

    DBusInterface {
        id: whisperfishApp
        service: "be.rubdos.whisperfish"
        path: "/be/rubdos/whisperfish/captcha"
        iface: "be.rubdos.whisperfish.captcha"
    }

    ConfigurationValue {
        key: "/apps/harbour-whisperfish/captchaType"
        Component.onCompleted: {
            if(value === "registration") {
                webView.url = "https://signalcaptchas.org/registration/generate"
            } else if(value === "challenge") {
                webView.url = "https://signalcaptchas.org/challenge/generate"
            } else {
                console.warn("Invalid captcha type - defaulting to challenge")
                webView.url = "https://signalcaptchas.org/challenge/generate"
            }
        }
    }

    Timer {
        id: closeTimer
        interval: 750
        running: false
        repeat: false
        onTriggered: Qt.quit()
    }

    PageHeader {
        id: header
        //: Registration captcha page title
        //% "Signal Captcha"
        title: qsTrId("whisperfish-signal-captcha")
    }

    Rectangle {
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
    }

    WebView {
        id: webView

        anchors {
            verticalCenter: parent.verticalCenter
            horizontalCenter: parent.horizontalCenter
        }

        // Capcha Format: aprox. 300px x 500x: 300/500 = 0.6; 500/300 = 1.666
        viewportWidth: parent.width
        viewportHeight: Math.min(parent.width*1.666, parent.height)
        width: viewportWidth
        height: viewportHeight

        active: true
        url: ""

        onViewInitialized: {
            webView.loadFrameScript(Qt.resolvedUrl("captchaframescript.js"));
            webView.addMessageListener("Whisperfish:CaptchaDone");
        }

        function filterUrl(uri) {
            var codeMatch = /^signalcaptcha:\/\/(.*)$/.exec(uri);
            if (!captchaCaptured && codeMatch !== null && codeMatch[1] != '') {
                captchaCaptured = true
                console.log("Captcha response parsed:", codeMatch[1]);
                complete(codeMatch[1]);
                return true;
            }
            return false;
        }

        property bool captchaSent: false

        function complete(code) {
            if(!captchaSent) {
                captchaSent = true
                whisperfishApp.call(
                    "handleCaptcha",
                    [code],
                    function () {
                        console.log("Captcha code sent!")
                        closeTimer.start()
                    },
                    function (error, message) {
                        console.log('Sending captcha code failed: ' + error + ' message: ' + message)
                        closeTimer.start()
                    }
                )
            }
        }

        onUrlChanged: {
            console.log("Url changed to: " + webView.url);
            if (filterUrl(url)) {
                webView.loadHtml("<html><head></head><body></body></html>");
            }
        }

        onRecvAsyncMessage: {
            if (message == "Whisperfish:CaptchaDone") {
                console.log("CaptchaDone Received!");
                complete(data.code);
            }
        }
    }
}
