package sdk.elastic.stream.client.route;

import java.util.Iterator;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class EndpointsTest {
    @Test
    public void testEndpointsWithSingleIpv4AndPort() {
        final Endpoints endpoints = new Endpoints("127.0.0.1:8080");
        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals(1, endpoints.getAddresses().size());
        final Address address = endpoints.getAddresses().iterator().next();
        Assertions.assertEquals("127.0.0.1", address.getHost());
        Assertions.assertEquals(8080, address.getPort());

        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals("ipv4:127.0.0.1:8080", endpoints.getFacade());
    }

    @Test
    public void testEndpointsWithSingleIpv4() {
        final Endpoints endpoints = new Endpoints("127.0.0.1");
        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals(1, endpoints.getAddresses().size());
        final Address address = endpoints.getAddresses().iterator().next();
        Assertions.assertEquals("127.0.0.1", address.getHost());
        Assertions.assertEquals(80, address.getPort());

        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals("ipv4:127.0.0.1:80", endpoints.getFacade());
    }

    @Test
    public void testEndpointsWithMultipleIpv4() {
        final Endpoints endpoints = new Endpoints("127.0.0.1:8080;127.0.0.2:8081");
        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals(2, endpoints.getAddresses().size());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address0 = iterator.next();
        Assertions.assertEquals("127.0.0.1", address0.getHost());
        Assertions.assertEquals(8080, address0.getPort());

        final Address address1 = iterator.next();
        Assertions.assertEquals("127.0.0.2", address1.getHost());
        Assertions.assertEquals(8081, address1.getPort());

        Assertions.assertEquals(AddressScheme.IPv4, endpoints.getScheme());
        Assertions.assertEquals("ipv4:127.0.0.1:8080,127.0.0.2:8081", endpoints.getFacade());
    }

    @Test
    public void testEndpointsWithSingleIpv6() {
        final Endpoints endpoints = new Endpoints("1050:0000:0000:0000:0005:0600:300c:326b:8080");
        Assertions.assertEquals(AddressScheme.IPv6, endpoints.getScheme());
        Assertions.assertEquals(1, endpoints.getAddresses().size());
        final Address address = endpoints.getAddresses().iterator().next();
        Assertions.assertEquals("1050:0000:0000:0000:0005:0600:300c:326b", address.getHost());
        Assertions.assertEquals(8080, address.getPort());

        Assertions.assertEquals(AddressScheme.IPv6, endpoints.getScheme());
        Assertions.assertEquals("ipv6:1050:0000:0000:0000:0005:0600:300c:326b:8080", endpoints.getFacade());
    }

    @Test
    public void testEndpointsWithMultipleIpv6() {
        final Endpoints endpoints = new Endpoints("1050:0000:0000:0000:0005:0600:300c:326b:8080;" +
            "1050:0000:0000:0000:0005:0600:300c:326c:8081");
        Assertions.assertEquals(AddressScheme.IPv6, endpoints.getScheme());
        Assertions.assertEquals(2, endpoints.getAddresses().size());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address0 = iterator.next();
        Assertions.assertEquals("1050:0000:0000:0000:0005:0600:300c:326b", address0.getHost());
        Assertions.assertEquals(8080, address0.getPort());

        final Address address1 = iterator.next();
        Assertions.assertEquals("1050:0000:0000:0000:0005:0600:300c:326c", address1.getHost());
        Assertions.assertEquals(8081, address1.getPort());

        Assertions.assertEquals(AddressScheme.IPv6, endpoints.getScheme());
        Assertions.assertEquals("ipv6:1050:0000:0000:0000:0005:0600:300c:326b:8080," +
            "1050:0000:0000:0000:0005:0600:300c:326c:8081", endpoints.getFacade());
    }

    @Test
    public void testEndpointsWithDomain() {
        final Endpoints endpoints = new Endpoints("example.apache.org");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(80, address.getPort());
    }

    @Test
    public void testEndpointsWithDomainAndPort() {
        final Endpoints endpoints = new Endpoints("example.apache.org:8081");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(8081, address.getPort());
    }

    @Test
    @SuppressWarnings("HttpUrlsUsage")
    public void testEndpointsWithDomainAndHttpPrefix() {
        final Endpoints endpoints = new Endpoints("http://example.apache.org");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(80, address.getPort());
    }

    @Test
    public void testEndpointsWithDomainAndHttpsPrefix() {
        final Endpoints endpoints = new Endpoints("https://example.apache.org");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(80, address.getPort());
    }

    @Test
    @SuppressWarnings("HttpUrlsUsage")
    public void testEndpointsWithDomainPortAndHttpPrefix() {
        final Endpoints endpoints = new Endpoints("http://example.apache.org:8081");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(8081, address.getPort());
    }

    @Test
    public void testEndpointsWithDomainPortAndHttpsPrefix() {
        final Endpoints endpoints = new Endpoints("https://example.apache.org:8081");
        Assertions.assertEquals(AddressScheme.DOMAIN_NAME, endpoints.getScheme());
        final Iterator<Address> iterator = endpoints.getAddresses().iterator();

        final Address address = iterator.next();
        Assertions.assertEquals("example.apache.org", address.getHost());
        Assertions.assertEquals(8081, address.getPort());
    }
}