
.. _notifications:

Notifications
=============

Overview
--------

* Proxmox Datacenter Manager emits :ref:`notification_events` for noteworthy events. Every event
  carries metadata such as a severity level, a type and further event specific fields.
* :ref:`notification_matchers` route an event to one or more notification targets. A matcher can
  use match rules to route selectively, based on the metadata of an event.
* :ref:`notification_targets` are the destinations an event is routed to.

Targets and matchers are configured under *Configuration → Notifications*. The
configuration is stored in ``/etc/proxmox-datacenter-manager/notifications.cfg``. Sensitive
options such as passwords or authentication tokens are kept in
``/etc/proxmox-datacenter-manager/notifications-priv.cfg``, which is only readable by ``root``.

.. NOTE:: No targets or matchers are configured by default, which means no notifications are sent
   until you create at least one of each.

.. _notification_targets:

Notification Targets
--------------------

Sendmail
^^^^^^^^
Uses the system's ``sendmail`` binary to send emails to a list of recipients. The sender address
defaults to the ``email-from`` setting of the node configuration, falling back to ``root``.

SMTP
^^^^
Sends emails directly to an SMTP server, without a local mail transfer agent. Unlike the sendmail
target, this does not require a working local mail queue, but mails will not be retried if the
server is temporarily unreachable.

Gotify
^^^^^^
Sends a message to a `Gotify <https://gotify.net/>`_ server. Note that Gotify does not
support message titles and bodies in HTML, so the plaintext body is used.

Webhook
^^^^^^^
Sends an arbitrary, fully customizable HTTP request. The URL, headers and body support template
expansion using `handlebars <https://handlebarsjs.com/>`_ syntax, so this target can integrate
with services that have no dedicated target type - see :ref:`notifications_telegram` below.

Tokens and other credentials can be stored as *secrets*. Secrets are kept in
``notifications-priv.cfg``, are never returned by the API and can be referenced from any template
as ``{{ secrets.<name> }}``.

.. _notification_matchers:

Notification Matchers
---------------------

A matcher routes an event to one or more targets. Without a matching matcher an event is
discarded, and an event matched by several matchers is sent to the union of their targets.

A matcher can restrict which events it accepts:

:Match Severity: Matches if the event has one of the selected severities.

:Match Field: Matches on the event metadata, written as ``<field>=<value>`` - for example
   ``type=remote-unreachable``. Multiple values for the same field are treated as a list of
   accepted values.

:Match Calendar: Matches if the event occurs within the given timespan, for example
   ``mon..fri 8:00-17:00``.

The *Match if* setting selects whether all or any of the configured rules have to match, and
*Invert Match* inverts the overall result.

.. _notification_events:

Notification Events
-------------------

The following events are emitted. The ``type`` field, as well as every listed metadata field, can
be used in ``match-field`` rules.

:task-result: A locally scheduled task finished. The severity is ``info`` when the task
   succeeded, ``warning`` when it finished with warnings and ``error`` when it failed. Additional
   metadata fields: ``job-type``, ``job-id``, ``hostname``.

:remote-unreachable: A remote host that was previously reachable started to fail. The severity
   is ``error``. Additional metadata fields: ``remote``, ``hostname``.

.. NOTE:: The remote reachability event fires once per outage, when a previously reachable remote
   host starts failing - not on every connection retry.

.. _notifications_telegram:

Telegram Notifications via Webhook
----------------------------------

Proxmox Datacenter Manager has no dedicated Telegram target. A generic *Webhook* target pointed at
the Telegram Bot API works instead:

#. Create a bot with `@BotFather <https://t.me/BotFather>`_ and note the bot token it returns (it
   looks like ``123456789:AAExampleTokenExampleTokenExample``).

#. Determine the chat ID to send to, for example by sending a message to your bot and then opening
   ``https://api.telegram.org/bot<token>/getUpdates``.

#. Add a *Webhook* target with method ``POST`` and the following URL, substituting your own token
   and chat ID:

   .. code-block:: text

      https://api.telegram.org/bot<token>/sendMessage?chat_id=<chat-id>&text={{ url-encode title }}

   Passing the message through the ``url-encode`` helper is required, because the text becomes
   part of the query string.

#. Add a matcher that routes the events you are interested in to this target, then use the *Test*
   button on the target to verify the setup.

To keep the bot token out of the world-readable ``notifications.cfg``, store it as a secret named
``token`` and reference it as ``{{ secrets.token }}`` in the URL instead of pasting it in
literally. Secrets, custom headers and a custom request body currently have to be set through the
API rather than the web interface, for example:

.. code-block:: console

   # curl -XPUT --header 'Content-Type: application/json' \
       --data '{"secret":[{"name":"token","value":"<base64-encoded-token>"}]}' \
       'https://<pdm-host>:8443/api2/json/config/notifications/webhook/<target-name>'

.. NOTE:: Secret and header values are expected to be base64 encoded by the API.
